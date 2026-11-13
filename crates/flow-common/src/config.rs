//! Versioned TOML config with forward migrations.
//!
//! A config file carries a top-level `version = N`. A [`Migrations`] list
//! upgrades older files one step at a time (`migrations[i]` turns version
//! `i` into `i + 1`) before deserialisation. Files newer than we understand are
//! rejected rather than silently truncated.

use crate::{FlowError, Result};
use serde::{de::DeserializeOwned, Serialize};
use std::path::Path;

pub type Migration = fn(&mut toml::Table);

/// Parse `text`, run needed migrations, deserialise into `T`.
/// Returns the value and whether a migration happened.
pub fn parse_versioned<T: DeserializeOwned>(
    text: &str,
    current: u32,
    migrations: &[Migration],
) -> Result<(T, bool)> {
    let mut table: toml::Table = text.parse()?;
    let mut version = table
        .get("version")
        .and_then(|v| v.as_integer())
        .unwrap_or(0);
    if version < 0 || version as u64 > u64::from(current) {
        return Err(FlowError::Config(format!(
            "config version {version} is newer than supported {current}"
        )));
    }
    let migrated = (version as u32) < current;
    while (version as u32) < current {
        if let Some(m) = migrations.get(version as usize) {
            m(&mut table);
        }
        version += 1;
    }
    table.insert("version".into(), toml::Value::Integer(i64::from(current)));
    let value = T::deserialize(toml::Value::Table(table))?;
    Ok((value, migrated))
}

/// Load a config file. Missing file => `T::default()`.
/// If a migration ran, the upgraded file is written back.
pub fn load<T>(path: &Path, current: u32, migrations: &[Migration]) -> Result<T>
where
    T: DeserializeOwned + Serialize + Default,
{
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(T::default()),
        Err(e) => return Err(e.into()),
    };
    let (value, migrated) = parse_versioned::<T>(&text, current, migrations)?;
    if migrated {
        save(path, current, &value)?;
    }
    Ok(value)
}

/// Serialise `value` with a leading `version` key and write atomically.
pub fn save<T: Serialize>(path: &Path, current: u32, value: &T) -> Result<()> {
    let mut table = match toml::Value::try_from(value)? {
        toml::Value::Table(t) => t,
        _ => return Err(FlowError::Config("config root must be a table".into())),
    };
    table.remove("version");
    let body = toml::to_string_pretty(&table)?;
    let text = format!("version = {current}\n{body}");
    crate::paths::write_atomic(path, text.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
    #[serde(default)]
    struct Cfg {
        timeout_s: u32,
        name: String,
    }

    // v0 called it `timeout`.
    fn m0(t: &mut toml::Table) {
        if let Some(v) = t.remove("timeout") {
            t.insert("timeout_s".into(), v);
        }
    }

    #[test]
    fn migrates_old() {
        let (c, migrated) = parse_versioned::<Cfg>("timeout = 5\nname='a'", 1, &[m0]).unwrap();
        assert!(migrated);
        assert_eq!(
            c,
            Cfg {
                timeout_s: 5,
                name: "a".into()
            }
        );
    }

    #[test]
    fn rejects_newer() {
        assert!(parse_versioned::<Cfg>("version = 9", 1, &[m0]).is_err());
    }

    #[test]
    fn missing_is_default_and_save_load() {
        let d = std::env::temp_dir().join(format!("flow-cfg-{}", std::process::id()));
        let p = d.join("x.toml");
        assert_eq!(load::<Cfg>(&p, 1, &[m0]).unwrap(), Cfg::default());
        let c = Cfg {
            timeout_s: 7,
            name: "z".into(),
        };
        save(&p, 1, &c).unwrap();
        assert_eq!(load::<Cfg>(&p, 1, &[m0]).unwrap(), c);
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn old_file_rewritten_after_migration() {
        let d = std::env::temp_dir().join(format!("flow-cfg2-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("x.toml");
        std::fs::write(&p, "timeout = 3\n").unwrap();
        let c: Cfg = load(&p, 1, &[m0]).unwrap();
        assert_eq!(c.timeout_s, 3);
        assert!(std::fs::read_to_string(&p).unwrap().contains("version = 1"));
        std::fs::remove_dir_all(d).unwrap();
    }
}
