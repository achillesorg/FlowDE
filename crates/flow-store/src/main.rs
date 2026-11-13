//! flow-store daemon: search (apt-cache local; flatpak needs its remote),
//! job queue for install/remove. Installs are privileged (pkexec / flatpak
//! polkit) and need network: only run on explicit request, never in tests.

use flow_common::ipc::{self, EventBus, Handler, RpcError};
use flow_store::*;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, BufReader};

struct Shared {
    q: Mutex<Queue>,
    bus: EventBus,
}

async fn run(cmd: &[&str]) -> Option<String> {
    let o = tokio::process::Command::new(cmd[0])
        .args(&cmd[1..])
        .stdin(std::process::Stdio::null())
        .output()
        .await
        .ok()?;
    o.status
        .success()
        .then(|| String::from_utf8_lossy(&o.stdout).into_owned())
}

async fn installed_apt() -> HashSet<String> {
    run(&["dpkg-query", "-f", "${Package}\n", "-W"])
        .await
        .map(|s| s.lines().map(String::from).collect())
        .unwrap_or_default()
}

#[derive(Deserialize)]
struct SearchP {
    query: String,
    #[serde(default)]
    flatpak: bool,
}
#[derive(Deserialize)]
struct OpP {
    pkg: Pkg,
}
#[derive(Deserialize)]
struct IdP {
    id: u32,
}

async fn worker(sh: Arc<Shared>) {
    loop {
        let job = sh.q.lock().unwrap().start_next().cloned_job();
        let Some((id, argv)) = job else {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            continue;
        };
        sh.bus
            .emit("store.job", json!({"id": id, "state": "running"}));
        let code = match tokio::process::Command::new(&argv[0])
            .args(&argv[1..])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            Ok(mut ch) => {
                if let Some(out) = ch.stdout.take() {
                    let mut lines = BufReader::new(out).lines();
                    while let Ok(Some(l)) = lines.next_line().await {
                        sh.q.lock().unwrap().log(id, &l);
                    }
                }
                ch.wait().await.ok().and_then(|s| s.code())
            }
            Err(e) => {
                sh.q.lock().unwrap().log(id, &e.to_string());
                None
            }
        };
        sh.q.lock().unwrap().finish(id, code);
        sh.bus.emit("store.job", json!({"id": id, "state": if code == Some(0) {"done"} else {"failed"}, "exit_code": code}));
    }
}

trait ClonedJob {
    fn cloned_job(self) -> Option<(u32, Vec<String>)>;
}
impl ClonedJob for Option<&Job> {
    fn cloned_job(self) -> Option<(u32, Vec<String>)> {
        let j = self?;
        command_for(j.action, &j.pkg).ok().map(|a| (j.id, a))
    }
}

fn main() {
    flow_common::daemon::run_daemon("flow-store", |bus| async {
        let sh = Arc::new(Shared {
            q: Mutex::new(Queue::default()),
            bus,
        });
        let s2 = sh.clone();
        let h: Arc<dyn Handler> = Arc::new(move |m: String, p: Value| {
            let sh = s2.clone();
            ipc::boxed(async move {
                match m.as_str() {
                    "store.search" => {
                        let p: SearchP = ipc::params(p)?;
                        let q = p.query.trim().to_string();
                        if q.is_empty() || q.starts_with('-') || q.len() > 100 {
                            return Err(RpcError::invalid(
                                "query must be 1..100 chars and not start with '-'",
                            ));
                        }
                        let inst = installed_apt().await;
                        let mut res = run(&["apt-cache", "search", "--", &q])
                            .await
                            .map(|o| parse_apt_search(&o, &inst, 60))
                            .unwrap_or_default();
                        if p.flatpak {
                            let fi: HashSet<String> =
                                run(&["flatpak", "list", "--app", "--columns=application"])
                                    .await
                                    .map(|s| s.lines().map(String::from).collect())
                                    .unwrap_or_default();
                            if let Some(o) = run(&[
                                "flatpak",
                                "search",
                                "--columns=application,name,description,version,remotes",
                                &q,
                            ])
                            .await
                            {
                                res.extend(parse_flatpak_search(&o, &fi));
                            }
                        }
                        ipc::ok(res)
                    }
                    "store.install" | "store.remove" => {
                        let p: OpP = ipc::params(p)?;
                        let a = if m.ends_with("install") {
                            Action::Install
                        } else {
                            Action::Remove
                        };
                        let id =
                            sh.q.lock()
                                .unwrap()
                                .enqueue(a, p.pkg)
                                .map_err(RpcError::invalid)?;
                        Ok(json!({"job": id}))
                    }
                    "store.jobs" => ipc::ok(&sh.q.lock().unwrap().jobs),
                    "store.cancel" => {
                        let p: IdP = ipc::params(p)?;
                        Ok(json!(sh.q.lock().unwrap().cancel(p.id)))
                    }
                    m => Err(RpcError::unknown_method(m)),
                }
            })
        });
        let w = tokio::spawn(worker(sh.clone()));
        Ok((h, vec![w]))
    });
}
