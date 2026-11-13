//! Inference runtime abstraction.

use serde::Serialize;
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct GenRequest {
    pub prompt: String,
    pub max_tokens: u32,
    pub temperature: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum BackendError {
    Unavailable(String),
    Cancelled,
    Failed(String),
}

/// A blocking text-generation backend. Implementations: [`MockBackend`] (tests),
/// [`CliBackend`] (llama.cpp CLI). TODO(feature): llama.cpp server over a
/// loopback socket; in-process backend (llama-cpp bindings) behind this trait.
pub trait Backend: Send + Sync {
    fn name(&self) -> &str;
    fn available(&self) -> bool;
    fn generate(
        &self,
        req: &GenRequest,
        cancel: &AtomicBool,
        on_token: &mut dyn FnMut(&str),
    ) -> Result<(), BackendError>;
    /// Free model memory (in-process backends). Default: nothing to free.
    fn unload(&self) {}
}

/// Scripted backend: emits `script` word by word, or echoes the last user turn.
pub struct MockBackend {
    pub script: Option<String>,
}

impl Backend for MockBackend {
    fn name(&self) -> &str {
        "mock"
    }
    fn available(&self) -> bool {
        true
    }
    fn generate(
        &self,
        req: &GenRequest,
        cancel: &AtomicBool,
        on_token: &mut dyn FnMut(&str),
    ) -> Result<(), BackendError> {
        let text = self
            .script
            .clone()
            .unwrap_or_else(|| format!("echo: {}", last_user_turn(&req.prompt)));
        for w in text.split_inclusive(' ').take(req.max_tokens as usize) {
            if cancel.load(Ordering::Relaxed) {
                return Err(BackendError::Cancelled);
            }
            on_token(w);
        }
        Ok(())
    }
}

fn last_user_turn(prompt: &str) -> String {
    prompt
        .rsplit("<|im_start|>user\n")
        .next()
        .and_then(|s| s.split("<|im_end|>").next())
        .unwrap_or("")
        .to_string()
}

/// Drives a llama.cpp style CLI. `args` may contain `{model}`, `{prompt}`,
/// `{max_tokens}`, `{temp}`; substitution is per argument, never through a shell.
pub struct CliBackend {
    pub program: String,
    pub args: Vec<String>,
    pub model_path: String,
}

impl CliBackend {
    pub fn argv(&self, req: &GenRequest) -> Vec<String> {
        self.args
            .iter()
            .map(|a| {
                a.replace("{model}", &self.model_path)
                    .replace("{prompt}", &req.prompt)
                    .replace("{max_tokens}", &req.max_tokens.to_string())
                    .replace("{temp}", &format!("{:.2}", req.temperature))
            })
            .collect()
    }
}

impl Backend for CliBackend {
    fn name(&self) -> &str {
        "cli"
    }
    fn available(&self) -> bool {
        std::env::var_os("PATH")
            .is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(&self.program).is_file()))
            || std::path::Path::new(&self.program).is_file()
    }
    fn generate(
        &self,
        req: &GenRequest,
        cancel: &AtomicBool,
        on_token: &mut dyn FnMut(&str),
    ) -> Result<(), BackendError> {
        if !self.available() {
            return Err(BackendError::Unavailable(format!(
                "{} not found",
                self.program
            )));
        }
        let mut child = Command::new(&self.program)
            .args(self.argv(req))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| BackendError::Failed(e.to_string()))?;
        let mut out = child.stdout.take().expect("piped");
        let child = Arc::new(Mutex::new(child));
        let done = Arc::new(AtomicBool::new(false));
        let status = std::thread::scope(|s| {
            let (c2, d2) = (child.clone(), done.clone());
            s.spawn(move || {
                while !d2.load(Ordering::Relaxed) {
                    if cancel.load(Ordering::Relaxed) {
                        let _ = c2.lock().unwrap().kill();
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
            });
            let mut buf = [0u8; 256];
            let mut pending: Vec<u8> = vec![];
            while let Ok(n) = out.read(&mut buf) {
                if n == 0 {
                    break;
                }
                pending.extend_from_slice(&buf[..n]);
                // Emit only complete UTF-8 sequences.
                let valid = match std::str::from_utf8(&pending) {
                    Ok(s) => s.len(),
                    Err(e) => e.valid_up_to(),
                };
                if valid > 0 {
                    on_token(std::str::from_utf8(&pending[..valid]).unwrap_or(""));
                    pending.drain(..valid);
                }
            }
            done.store(true, Ordering::Relaxed);
            child.lock().unwrap().wait()
        });
        if cancel.load(Ordering::Relaxed) {
            return Err(BackendError::Cancelled);
        }
        match status {
            Ok(s) if s.success() => Ok(()),
            Ok(s) => Err(BackendError::Failed(format!("exit {s}"))),
            Err(e) => Err(BackendError::Failed(e.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(p: &str) -> GenRequest {
        GenRequest {
            prompt: p.into(),
            max_tokens: 100,
            temperature: 0.7,
        }
    }

    #[test]
    fn mock_streams_and_cancels() {
        let b = MockBackend {
            script: Some("hello brave new world".into()),
        };
        let mut got = String::new();
        b.generate(&req("x"), &AtomicBool::new(false), &mut |t| got.push_str(t))
            .unwrap();
        assert_eq!(got, "hello brave new world");
        let cancel = AtomicBool::new(false);
        let mut n = 0;
        let r = b.generate(&req("x"), &cancel, &mut |_| {
            n += 1;
            cancel.store(true, Ordering::Relaxed);
        });
        assert_eq!(r, Err(BackendError::Cancelled));
        assert_eq!(n, 1);
        let e = MockBackend { script: None };
        let mut s = String::new();
        e.generate(
            &req("<|im_start|>user\nhi there<|im_end|>\n<|im_start|>assistant\n"),
            &AtomicBool::new(false),
            &mut |t| s.push_str(t),
        )
        .unwrap();
        assert_eq!(s, "echo: hi there");
    }

    #[test]
    fn cli_argv_substitution_is_per_argument() {
        let c = CliBackend {
            program: "llama-cli".into(),
            args: vec![
                "-m".into(),
                "{model}".into(),
                "-p".into(),
                "{prompt}".into(),
                "-n".into(),
                "{max_tokens}".into(),
            ],
            model_path: "/m/q.gguf".into(),
        };
        let v = c.argv(&req("a; rm -rf / $(x)"));
        assert_eq!(
            v,
            vec!["-m", "/m/q.gguf", "-p", "a; rm -rf / $(x)", "-n", "100"]
        );
    }

    #[cfg(unix)]
    #[test]
    fn cli_streams_stdout_with_echo() {
        let c = CliBackend {
            program: "echo".into(),
            args: vec!["{prompt}".into()],
            model_path: String::new(),
        };
        if !c.available() {
            return;
        }
        let mut got = String::new();
        c.generate(&req("héllo"), &AtomicBool::new(false), &mut |t| {
            got.push_str(t)
        })
        .unwrap();
        assert_eq!(got.trim(), "héllo");
        let missing = CliBackend {
            program: "no-such-llama-xyz".into(),
            args: vec![],
            model_path: String::new(),
        };
        assert!(matches!(
            missing.generate(&req("x"), &AtomicBool::new(false), &mut |_| {}),
            Err(BackendError::Unavailable(_))
        ));
    }
}
