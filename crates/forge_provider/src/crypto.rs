use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use base64::engine::general_purpose;
use base64::Engine as _;
use ed25519_dalek::{Signer, SigningKey};

pub struct Payload {
    pub payload: String,
    pub signature: String,
}

/// Cryptographic authentication module for ForgeProvider
/// Provides Ed25519 digital signatures for HTTP request authentication
#[derive(Clone)]
pub struct CryptoAuth {
    signing_key: SigningKey,
}

impl CryptoAuth {
    /// Create a new CryptoAuth instance with key loading from environment
    pub fn new(private_key: impl ToString) -> Result<Self> {
        let signing_key = Self::load_private_key(private_key)?;

        Ok(Self { signing_key })
    }

    /// Load private key from environment variable or generate for development
    fn load_private_key(private_key: impl ToString) -> Result<SigningKey> {
        let key_bytes = general_purpose::STANDARD
            .decode(private_key.to_string())
            .context("Failed to decode base64 private key from environment")?;

        let mut key_array = [0u8; 32];
        key_array.copy_from_slice(&key_bytes);

        Ok(SigningKey::from_bytes(&key_array))
    }

    /// Generate authentication headers with cryptographic signatures
    pub fn generate_payload(&self) -> Result<Payload> {
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

        Ok(Payload { payload: payload_b64, signature: signature_b64 })
    }
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::{Signature, Verifier};
    use pretty_assertions::assert_eq;

    use super::*;

    const TEST_PRIVATE_KEY: &str = "rMMSj0qvfi5O8S76CjgW2Q6K9NTx7Zrn0Swjryv0wgE=";

    #[test]
    fn test_crypto_auth_initialization() {
        let fixture = CryptoAuth::new(TEST_PRIVATE_KEY);
        let actual = fixture.is_ok();
        let expected = true;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_signature_verification_roundtrip() {
        let fixture = CryptoAuth::new(TEST_PRIVATE_KEY).unwrap();
        let headers = fixture.generate_payload().unwrap();

        // Extract components
        let payload_b64 = &headers.payload;
        let signature_b64 = &headers.signature;

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
