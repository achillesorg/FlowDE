//! XDG-style paths. Every root can be overridden through `FLOW_*` env vars,
//! which is what tests and the VM checks use.

use std::path::PathBuf;

fn env_path(var: &str) -> Option<PathBuf> {
    std::env::var_os(var)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

fn home() -> PathBuf {
    env_path("HOME").unwrap_or_else(|| PathBuf::from("/tmp"))
}

/// `$FLOW_CONFIG_DIR` or `$XDG_CONFIG_HOME/flowde` or `~/.config/flowde`.
pub fn config_dir() -> PathBuf {
    env_path("FLOW_CONFIG_DIR").unwrap_or_else(|| {
        env_path("XDG_CONFIG_HOME")
            .unwrap_or_else(|| home().join(".config"))
            .join("flowde")
    })
}

/// `$FLOW_STATE_DIR` or `$XDG_STATE_HOME/flowde` or `~/.local/state/flowde`.
pub fn state_dir() -> PathBuf {
    env_path("FLOW_STATE_DIR").unwrap_or_else(|| {
        env_path("XDG_STATE_HOME")
            .unwrap_or_else(|| home().join(".local/state"))
            .join("flowde")
    })
}

/// `$FLOW_RUNTIME_DIR` or `$XDG_RUNTIME_DIR/flowde` or `/tmp/flowde-<uid-ish>`.
pub fn runtime_dir() -> PathBuf {
    env_path("FLOW_RUNTIME_DIR").unwrap_or_else(|| match env_path("XDG_RUNTIME_DIR") {
        Some(p) => p.join("flowde"),
        None => {
            let user = std::env::var("USER").unwrap_or_else(|_| "user".into());
            std::env::temp_dir().join(format!("flowde-{user}"))
        }
    })
}

/// Control socket path of a daemon, e.g. `socket_path("flow-notify")`.
pub fn socket_path(daemon: &str) -> PathBuf {
    runtime_dir().join(format!("{daemon}.sock"))
}

/// Config file of a daemon: `<config>/<daemon>.toml`.
pub fn config_file(daemon: &str) -> PathBuf {
    config_dir().join(format!("{daemon}.toml"))
}

/// Create a directory (and parents) with mode 0700 on unix.
pub fn ensure_private_dir(path: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// Write a file atomically (tmp + rename) with mode 0600 on unix.
pub fn write_atomic(path: &std::path::Path, data: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, data)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
    }
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socket_under_runtime() {
        let p = socket_path("flow-x");
        assert!(p.ends_with("flow-x.sock"));
    }

    #[test]
    fn atomic_write_roundtrip() {
        let d = std::env::temp_dir().join(format!("flow-paths-{}", std::process::id()));
        let f = d.join("a/b.txt");
        write_atomic(&f, b"hi").unwrap();
        assert_eq!(std::fs::read(&f).unwrap(), b"hi");
        std::fs::remove_dir_all(d).unwrap();
    }
}
