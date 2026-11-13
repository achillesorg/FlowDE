//! Passcode storage: argon2id PHC string, mode-0600 file.

use argon2::password_hash::{
    rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
};
use argon2::{Algorithm, Argon2, Params as A2Params, Version};
use flow_common::{FlowError, Result};
use std::path::PathBuf;

pub const MIN_LEN: usize = 4;
pub const MAX_LEN: usize = 128;

#[derive(Debug, Clone, Copy)]
pub struct Params {
    pub m_kib: u32,
    pub t: u32,
    pub p: u32,
}

impl Params {
    /// OWASP-recommended argon2id minimum (19 MiB, t=2, p=1): fine on phones.
    pub fn standard() -> Self {
        Self {
            m_kib: 19 * 1024,
            t: 2,
            p: 1,
        }
    }
    /// For unit tests only.
    pub fn cheap() -> Self {
        Self {
            m_kib: 8,
            t: 1,
            p: 1,
        }
    }
    fn argon(&self) -> Result<Argon2<'static>> {
        let p = A2Params::new(self.m_kib, self.t, self.p, None)
            .map_err(|e| FlowError::Config(e.to_string()))?;
        Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, p))
    }
}

pub struct PasscodeStore {
    phc: Option<String>,
    path: Option<PathBuf>,
    params: Params,
}

impl PasscodeStore {
    pub fn in_memory(params: Params) -> Self {
        Self {
            phc: None,
            path: None,
            params,
        }
    }

    pub fn open(path: PathBuf, params: Params) -> Result<Self> {
        let phc = match std::fs::read_to_string(&path) {
            Ok(t) => {
                let t = t.trim().to_string();
                PasswordHash::new(&t)
                    .map_err(|e| FlowError::Config(format!("bad passcode file: {e}")))?;
                Some(t)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.into()),
        };
        Ok(Self {
            phc,
            path: Some(path),
            params,
        })
    }

    pub fn is_set(&self) -> bool {
        self.phc.is_some()
    }

    pub fn set(&mut self, pin: &str) -> Result<()> {
        let n = pin.chars().count();
        if !(MIN_LEN..=MAX_LEN).contains(&n) {
            return Err(FlowError::Invalid(format!(
                "passcode must be {MIN_LEN}..{MAX_LEN} characters"
            )));
        }
        let salt = SaltString::generate(&mut OsRng);
        let phc = self
            .params
            .argon()?
            .hash_password(pin.as_bytes(), &salt)
            .map_err(|e| FlowError::Config(e.to_string()))?
            .to_string();
        if let Some(p) = &self.path {
            flow_common::paths::write_atomic(p, phc.as_bytes())?;
        }
        self.phc = Some(phc);
        Ok(())
    }

    /// Constant-time (argon2 crate) verification. No passcode set => false.
    pub fn verify(&self, pin: &str) -> Result<bool> {
        let Some(phc) = &self.phc else {
            return Ok(false);
        };
        if pin.len() > MAX_LEN * 4 {
            return Ok(false);
        }
        let hash = PasswordHash::new(phc).map_err(|e| FlowError::Config(e.to_string()))?;
        // Verification uses the parameters embedded in the stored hash.
        Ok(Argon2::default()
            .verify_password(pin.as_bytes(), &hash)
            .is_ok())
    }

    pub fn clear(&mut self) -> Result<()> {
        self.phc = None;
        if let Some(p) = &self.path {
            match std::fs::remove_file(p) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }

    /// Stored PHC string (tests/inspection only).
    pub fn phc(&self) -> Option<&str> {
        self.phc.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_argon2id_and_salted() {
        let mut a = PasscodeStore::in_memory(Params::cheap());
        let mut b = PasscodeStore::in_memory(Params::cheap());
        a.set("1234").unwrap();
        b.set("1234").unwrap();
        assert!(a.phc().unwrap().starts_with("$argon2id$v=19$"));
        assert_ne!(a.phc(), b.phc()); // random salt
        assert!(!a.phc().unwrap().contains("1234"));
    }

    #[test]
    fn verify_and_length_rules() {
        let mut s = PasscodeStore::in_memory(Params::cheap());
        assert!(!s.verify("1234").unwrap());
        assert!(s.set("123").is_err());
        assert!(s.set(&"x".repeat(MAX_LEN + 1)).is_err());
        s.set("1234").unwrap();
        assert!(s.verify("1234").unwrap());
        assert!(!s.verify("1235").unwrap());
        assert!(!s.verify("").unwrap());
    }

    #[test]
    fn file_roundtrip_mode_0600() {
        let d = std::env::temp_dir().join(format!("kiln-lock-{}", std::process::id()));
        let p = d.join("pin");
        {
            let mut s = PasscodeStore::open(p.clone(), Params::cheap()).unwrap();
            s.set("424242").unwrap();
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&p).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        let mut s = PasscodeStore::open(p.clone(), Params::standard()).unwrap();
        assert!(s.verify("424242").unwrap());
        s.clear().unwrap();
        assert!(!p.exists());
        std::fs::write(&p, "garbage").unwrap();
        assert!(PasscodeStore::open(p, Params::cheap()).is_err());
        std::fs::remove_dir_all(d).unwrap();
    }
}
