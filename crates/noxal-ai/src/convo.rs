//! Conversation store: local JSON files, optional encryption at rest
//! (argon2id-derived key, ChaCha20-Poly1305), bounded size, one-call wipe.
//! There is no telemetry and nothing leaves the device.

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use password_hash::rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub text: String,
    pub time: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub messages: Vec<Message>,
}

pub const MAX_MESSAGES: usize = 500;
pub const MAX_CONVERSATIONS: usize = 100;
const MAGIC: &[u8; 4] = b"NXV1";

/// At-rest encryption key.
pub struct Vault {
    key: [u8; 32],
}

impl Vault {
    /// Derive from a passphrase and a 16-byte salt (argon2id; 19 MiB, t=2).
    pub fn derive(passphrase: &str, salt: &[u8; 16], cheap: bool) -> Result<Vault, String> {
        let params = if cheap {
            Params::new(8, 1, 1, Some(32))
        } else {
            Params::new(19 * 1024, 2, 1, Some(32))
        }
        .map_err(|e| e.to_string())?;
        let mut key = [0u8; 32];
        Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
            .hash_password_into(passphrase.as_bytes(), salt, &mut key)
            .map_err(|e| e.to_string())?;
        Ok(Vault { key })
    }

    pub fn seal(&self, plain: &[u8]) -> Vec<u8> {
        let c = ChaCha20Poly1305::new(Key::from_slice(&self.key));
        let mut nonce = [0u8; 12];
        OsRng.fill_bytes(&mut nonce);
        let ct = c
            .encrypt(Nonce::from_slice(&nonce), plain)
            .expect("encrypt");
        [MAGIC.as_slice(), &nonce, &ct].concat()
    }

    pub fn open(&self, data: &[u8]) -> Result<Vec<u8>, String> {
        if data.len() < 4 + 12 + 16 || &data[..4] != MAGIC {
            return Err("not an encrypted noxal file".into());
        }
        let c = ChaCha20Poly1305::new(Key::from_slice(&self.key));
        c.decrypt(Nonce::from_slice(&data[4..16]), &data[16..])
            .map_err(|_| "wrong passphrase or corrupted file".to_string())
    }
}

pub fn valid_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 40
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

pub struct Store {
    dir: PathBuf,
    vault: Option<Vault>,
}

impl Store {
    pub fn new(dir: PathBuf, vault: Option<Vault>) -> Store {
        Store { dir, vault }
    }

    fn file(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }

    pub fn save(&self, c: &Conversation) -> Result<(), String> {
        if !valid_id(&c.id) {
            return Err("bad conversation id".into());
        }
        let mut c = c.clone();
        if c.messages.len() > MAX_MESSAGES {
            let drop = c.messages.len() - MAX_MESSAGES;
            c.messages.drain(..drop);
        }
        let json = serde_json::to_vec(&c).map_err(|e| e.to_string())?;
        let data = match &self.vault {
            Some(v) => v.seal(&json),
            None => json,
        };
        flow_common::paths::write_atomic(&self.file(&c.id), &data).map_err(|e| e.to_string())?;
        self.prune();
        Ok(())
    }

    pub fn load(&self, id: &str) -> Result<Conversation, String> {
        if !valid_id(id) {
            return Err("bad conversation id".into());
        }
        let data = std::fs::read(self.file(id)).map_err(|e| e.to_string())?;
        let json = match &self.vault {
            Some(v) => v.open(&data)?,
            None => data,
        };
        serde_json::from_slice(&json).map_err(|e| e.to_string())
    }

    pub fn list(&self) -> Vec<String> {
        let mut v: Vec<(std::time::SystemTime, String)> = std::fs::read_dir(&self.dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let n = e
                    .file_name()
                    .to_string_lossy()
                    .strip_suffix(".json")?
                    .to_string();
                Some((e.metadata().ok()?.modified().ok()?, n))
            })
            .collect();
        v.sort_by_key(|a| std::cmp::Reverse(a.0));
        v.into_iter().map(|x| x.1).collect()
    }

    pub fn delete(&self, id: &str) -> bool {
        valid_id(id) && std::fs::remove_file(self.file(id)).is_ok()
    }

    /// Privacy wipe: removes every conversation file.
    pub fn delete_all(&self) -> usize {
        self.list().iter().filter(|id| self.delete(id)).count()
    }

    fn prune(&self) {
        for id in self.list().into_iter().skip(MAX_CONVERSATIONS) {
            self.delete(&id);
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("noxal-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn convo(id: &str) -> Conversation {
        Conversation {
            id: id.into(),
            title: "t".into(),
            messages: vec![Message {
                role: Role::User,
                text: "secret plan".into(),
                time: 1,
            }],
        }
    }

    #[test]
    fn plain_roundtrip_and_wipe() {
        let d = tmp("p");
        let s = Store::new(d.clone(), None);
        s.save(&convo("a")).unwrap();
        s.save(&convo("b")).unwrap();
        assert_eq!(s.load("a").unwrap().messages[0].text, "secret plan");
        assert_eq!(s.list().len(), 2);
        assert!(s.load("../etc").is_err());
        assert!(!s.delete("../x"));
        assert_eq!(s.delete_all(), 2);
        assert!(s.list().is_empty());
        std::fs::remove_dir_all(d).ok();
    }

    #[test]
    fn encrypted_at_rest() {
        let d = tmp("e");
        let salt = [7u8; 16];
        let s = Store::new(d.clone(), Some(Vault::derive("pass", &salt, true).unwrap()));
        s.save(&convo("a")).unwrap();
        let raw = std::fs::read(d.join("a.json")).unwrap();
        assert!(!String::from_utf8_lossy(&raw).contains("secret plan"));
        assert_eq!(&raw[..4], b"NXV1");
        assert_eq!(s.load("a").unwrap().id, "a");
        let wrong = Store::new(d.clone(), Some(Vault::derive("nope", &salt, true).unwrap()));
        assert!(wrong.load("a").is_err());
        // tampering is detected
        let mut bad = raw.clone();
        let n = bad.len();
        bad[n - 1] ^= 1;
        std::fs::write(d.join("a.json"), bad).unwrap();
        assert!(s.load("a").is_err());
        std::fs::remove_dir_all(d).ok();
    }

    #[test]
    fn bounds() {
        let d = tmp("b");
        let s = Store::new(d.clone(), None);
        let mut c = convo("big");
        c.messages = (0..MAX_MESSAGES + 20)
            .map(|i| Message {
                role: Role::User,
                text: i.to_string(),
                time: 0,
            })
            .collect();
        s.save(&c).unwrap();
        let l = s.load("big").unwrap();
        assert_eq!(l.messages.len(), MAX_MESSAGES);
        assert_eq!(l.messages[0].text, "20");
        assert!(valid_id("abc-1_2") && !valid_id("a/b") && !valid_id(""));
        std::fs::remove_dir_all(d).ok();
    }
}
