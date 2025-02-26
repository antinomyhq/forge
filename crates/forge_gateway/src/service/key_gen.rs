use rand::{thread_rng, Rng};

use crate::error::Result;

const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_=";

pub trait KeyGeneratorService: Send + Sync {
    fn generate_key(&self) -> Result<String>;
}

pub struct KeyGeneratorServiceImpl {
    pub key_length: usize,
    pub key_prefix: String,
}

impl KeyGeneratorServiceImpl {
    pub fn new<S: Into<String>>(key_length: usize, key_prefix: S) -> Self {
        Self { key_length, key_prefix: key_prefix.into() }
    }
}

impl KeyGeneratorService for KeyGeneratorServiceImpl {
    fn generate_key(&self) -> Result<String> {
        let mut rng = thread_rng();
        let key: String = (0..self.key_length)
            .map(|_| {
                let idx = rng.gen_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect();

        Ok(format!("{}-{}", self.key_prefix, key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_gen() {
        let key_gen = KeyGeneratorServiceImpl::new(32, "api-key");
        let key = key_gen.generate_key().unwrap();
        let prefix = "api-key";
        assert!(key.starts_with("api-key"));
        assert_eq!(key.len(), prefix.len() + 1 + 32);
    }
}
