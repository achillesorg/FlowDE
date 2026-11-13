//! Model registry. Points at GGUF files the user already has; never downloads.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelSpec {
    pub id: String,
    pub path: PathBuf,
    /// e.g. "qwen2.5-1.5b-instruct"; informational.
    #[serde(default)]
    pub family: String,
    #[serde(default)]
    pub quant: String,
    /// Approximate resident memory in MB (file size + KV cache).
    pub ram_mb: u64,
    #[serde(default = "d_ctx")]
    pub ctx: u32,
}
fn d_ctx() -> u32 {
    4096
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Registry {
    pub models: Vec<ModelSpec>,
    pub default: Option<String>,
}

impl Registry {
    pub fn validate(&self) -> Result<(), String> {
        for m in &self.models {
            if m.id.is_empty()
                || !m
                    .id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
            {
                return Err(format!("bad model id {:?}", m.id));
            }
            if !m.path.is_absolute()
                || m.path
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir))
            {
                return Err(format!(
                    "model path for {} must be absolute without '..'",
                    m.id
                ));
            }
            if m.path.extension().is_none_or(|e| e != "gguf") {
                return Err(format!("model {} is not a .gguf file", m.id));
            }
        }
        Ok(())
    }

    pub fn installed(&self) -> Vec<&ModelSpec> {
        self.models.iter().filter(|m| m.path.is_file()).collect()
    }

    /// Choose the largest installed model that fits `free_mb` (with headroom), preferring the default.
    pub fn pick(&self, free_mb: u64, headroom_mb: u64) -> Option<&ModelSpec> {
        let fits = |m: &ModelSpec| m.ram_mb + headroom_mb <= free_mb;
        let inst = self.installed();
        if let Some(d) = self
            .default
            .as_ref()
            .and_then(|d| inst.iter().find(|m| &m.id == d))
            .filter(|m| fits(m))
        {
            return Some(d);
        }
        inst.into_iter()
            .filter(|m| fits(m))
            .max_by_key(|m| m.ram_mb)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(id: &str, path: &str, ram: u64) -> ModelSpec {
        ModelSpec {
            id: id.into(),
            path: path.into(),
            family: "qwen".into(),
            quant: "q4_k_m".into(),
            ram_mb: ram,
            ctx: 4096,
        }
    }

    #[test]
    fn validation() {
        let mut r = Registry {
            models: vec![spec("a", "/m/a.gguf", 1000)],
            default: None,
        };
        assert!(r.validate().is_ok());
        r.models.push(spec("b", "rel.gguf", 1));
        assert!(r.validate().is_err());
        r.models[1] = spec("c", "/m/c.bin", 1);
        assert!(r.validate().is_err());
        r.models[1] = spec("d d", "/m/d.gguf", 1);
        assert!(r.validate().is_err());
        r.models[1] = spec("e", "/m/../x.gguf", 1);
        assert!(r.validate().is_err());
    }

    #[test]
    fn pick_by_ram_and_default() {
        let d = std::env::temp_dir().join(format!("noxal-models-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        for n in ["s", "m", "l"] {
            std::fs::write(d.join(format!("{n}.gguf")), "x").unwrap();
        }
        let mk = |n: &str, ram| spec(n, d.join(format!("{n}.gguf")).to_str().unwrap(), ram);
        let mut r = Registry {
            models: vec![
                mk("s", 900),
                mk("m", 1800),
                mk("l", 4500),
                spec("ghost", "/nope/g.gguf", 100),
            ],
            default: None,
        };
        assert_eq!(r.pick(3000, 512).unwrap().id, "m");
        assert_eq!(r.pick(10_000, 512).unwrap().id, "l");
        assert!(r.pick(1000, 512).is_none());
        r.default = Some("s".into());
        assert_eq!(r.pick(10_000, 512).unwrap().id, "s");
        assert_eq!(r.installed().len(), 3);
        std::fs::remove_dir_all(d).unwrap();
    }
}
