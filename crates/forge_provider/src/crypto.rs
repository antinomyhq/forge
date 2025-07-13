use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use base64::engine::general_purpose;
use base64::Engine as _;
use ed25519_dalek::{Signer, SigningKey};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

/// Cryptographic authentication module for ForgeProvider
/// Provides Ed25519 digital signatures for HTTP request authentication
#[derive(Clone)]
pub struct CryptoAuth {
    signing_key: SigningKey,
}

impl CryptoAuth {
    /// Create a new CryptoAuth instance with key loading from environment
    pub fn new() -> Result<Self> {
        let signing_key = Self::load_private_key()?;

        Ok(Self { signing_key })
    }

    /// Load private key from environment variable or generate for development
    fn load_private_key() -> Result<SigningKey> {
        // Try to load from environment variable first
        let key_b64 = obfstr::obfstr!(match option_env!("FORGE_PRIVATE_KEY") {
            Some(key) => key,
            None => "",
        })
        .to_string();
        let key_bytes = general_purpose::STANDARD
            .decode(key_b64)
            .context("Failed to decode base64 private key from environment")?;

        let mut key_array = [0u8; 32];
        key_array.copy_from_slice(&key_bytes);

        Ok(SigningKey::from_bytes(&key_array))
    }

    /// Generate authentication headers with cryptographic signatures
    pub fn generate_auth_headers(&self) -> Result<HeaderMap> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("Failed to get current timestamp")?
            .as_millis();

        // Create nonce using timestamp and a counter
        let nonce = format!("{}-{}", timestamp, rand::random::<u32>());

        // Create the payload to be signed
        let payload = format!("forge-auth:{timestamp}:{nonce}");

        // Create signature
        let signature = self.signing_key.sign(payload.as_bytes());

        // Encode components for headers
        let payload_b64 = general_purpose::STANDARD.encode(&payload);
        let signature_b64 = general_purpose::STANDARD.encode(signature.to_bytes());

        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::try_from("X-Forge-Auth-Payload")?,
            HeaderValue::from_str(&payload_b64)?,
        );
        headers.insert(
            HeaderName::try_from("X-Forge-Auth-Signature")?,
            HeaderValue::from_str(&signature_b64)?,
        );

        Ok(headers)
    }
}

impl Default for CryptoAuth {
    fn default() -> Self {
        Self::new().expect("Failed to initialize cryptographic authentication")
    }
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::{Signature, Verifier};
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_crypto_auth_initialization() {
        let fixture = CryptoAuth::new();
        let actual = fixture.is_ok();
        let expected = true;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_generate_auth_headers() {
        let fixture = CryptoAuth::new().unwrap();
        let actual = fixture.generate_auth_headers();

        assert!(actual.is_ok());
        let headers = actual.unwrap();

        // Verify all required headers are present
        assert!(headers.contains_key("X-Forge-Auth-Payload"));
        assert!(headers.contains_key("X-Forge-Auth-Signature"));
        assert!(headers.contains_key("X-Forge-Auth-Version"));

        // Verify version is correct
        assert_eq!(headers.get("X-Forge-Auth-Version").unwrap(), "1");
    }

    #[test]
    fn test_signature_verification_roundtrip() {
        let fixture = CryptoAuth::new().unwrap();
        let headers = fixture.generate_auth_headers().unwrap();

        // Extract components
        let payload_b64 = headers.get("X-Forge-Auth-Payload").unwrap();
        let signature_b64 = headers.get("X-Forge-Auth-Signature").unwrap();

        // Decode components
        let payload = general_purpose::STANDARD.decode(payload_b64).unwrap();
        let signature_bytes = general_purpose::STANDARD.decode(signature_b64).unwrap();

        // Verify signature using the public key from the fixture
        let signature = Signature::from_slice(&signature_bytes).unwrap();

        let actual = fixture
            .signing_key
            .verifying_key()
            .verify(&payload, &signature)
            .is_ok();
        let expected = true;
        assert_eq!(actual, expected);
    }
}
