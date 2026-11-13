use flow_cli::*;
use flow_common::ipc::Client;
use serde_json::Value;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let json_out = args.iter().any(|a| a == "--json");
    let args: Vec<String> = args.into_iter().filter(|a| a != "--json").collect();
    let cmd = match parse(&args) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("flow: {e}");
            std::process::exit(2);
        }
    };
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let code = rt.block_on(run(cmd, json_out));
    std::process::exit(code);
}

async fn run(cmd: Cmd, json_out: bool) -> i32 {
    match cmd {
        Cmd::Help => {
            println!("{HELP}");
            0
        }
        Cmd::Status => {
            let mut down = 0;
            println!(
                "{:<18} {:<6} {:>8} {:>9}",
                "DAEMON", "STATE", "UPTIME", "RSS"
            );
            for d in DAEMONS {
                match flow_common::ipc::call_once(d, "health", Value::Null).await {
                    Ok(h) => println!(
                        "{:<18} {:<6} {:>7}s {:>7} kB",
                        d,
                        "up",
                        h["uptime_s"].as_u64().unwrap_or(0),
                        h["rss_kb"]
                            .as_u64()
                            .map(|v| v.to_string())
                            .unwrap_or_else(|| "?".into())
                    ),
                    Err(_) => {
                        down += 1;
                        println!("{d:<18} {:<6}", "down");
                    }
                }
            }
            i32::from(down == DAEMONS.len())
        }
        Cmd::Call {
            daemon,
            method,
            params,
        }
        | Cmd::CallText {
            daemon,
            method,
            params,
        } => {
            let mut c = match Client::connect(&flow_common::paths::socket_path(&daemon)).await {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("flow: {daemon} is not running ({e})");
                    return 1;
                }
            };
            match c.call(&method, params).await {
                Ok(Value::String(s)) if !json_out => {
                    println!("{s}");
                    0
                }
                Ok(v) => {
                    println!(
                        "{}",
                        if json_out {
                            v.to_string()
                        } else {
                            serde_json::to_string_pretty(&v).unwrap_or_default()
                        }
                    );
                    0
                }
                Err(e) => {
                    eprintln!("flow: {e}");
                    1
                }
            }
        }
    }
}
