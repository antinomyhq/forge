use base64::Engine;
/// PKCE (Proof Key for Code Exchange) utilities for OAuth security
///
/// PKCE is a security extension for OAuth that prevents authorization code
/// interception attacks. It uses a code verifier and code challenge pair.
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::Rng;
use sha2::{Digest, Sha256};

const CODE_VERIFIER_LENGTH: usize = 64;
const STATE_LENGTH: usize = 32;

/// Generates a random code verifier for PKCE
///
/// The code verifier is a cryptographically random string between 43-128
/// characters using the characters [A-Z], [a-z], [0-9], "-", ".", "_", "~"
///
/// # Returns
///
/// A 64-character random string suitable for use as a PKCE code verifier
pub fn generate_code_verifier() -> String {
    let mut rng = rand::thread_rng();
    let charset: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";

    (0..CODE_VERIFIER_LENGTH)
        .map(|_| {
            let idx = rng.gen_range(0..charset.len());
            charset[idx] as char
        })
        .collect()
}

/// Generates a code challenge from a code verifier using S256 method
///
/// The code challenge is created by:
/// 1. Computing SHA-256 hash of the code verifier
/// 2. Base64-url encoding the hash (without padding)
///
/// # Arguments
///
/// * `verifier` - The code verifier string
///
/// # Returns
///
/// Base64-url encoded SHA-256 hash of the verifier
///
/// # Errors
///
/// Returns error if encoding fails (should never happen)
pub fn generate_code_challenge(verifier: &str) -> anyhow::Result<String> {
    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let hash = hasher.finalize();

    Ok(URL_SAFE_NO_PAD.encode(hash))
}

/// Generates a random state parameter for CSRF protection
///
/// The state parameter is used to prevent CSRF attacks by ensuring
/// the authorization response corresponds to the original request.
///
/// # Returns
///
/// A 32-character random hexadecimal string
pub fn generate_state() -> String {
    let mut rng = rand::thread_rng();
    let mut bytes = [0u8; STATE_LENGTH];
    rng.fill(&mut bytes);
    hex::encode(bytes)
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_generate_code_verifier_length() {
        let verifier = generate_code_verifier();
        assert_eq!(verifier.len(), CODE_VERIFIER_LENGTH);
    }

    #[test]
    fn test_generate_code_verifier_charset() {
        let verifier = generate_code_verifier();
        let valid_chars = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";

        for ch in verifier.chars() {
            assert!(
                valid_chars.contains(ch),
                "Invalid character '{}' in verifier",
                ch
            );
        }
    }

    #[test]
    fn test_generate_code_verifier_randomness() {
        let verifier1 = generate_code_verifier();
        let verifier2 = generate_code_verifier();

        // Extremely unlikely to be equal
        assert_ne!(verifier1, verifier2);
    }

    #[test]
    fn test_generate_code_challenge() {
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let challenge = generate_code_challenge(verifier).unwrap();

        // Known test vector from RFC 7636
        assert_eq!(challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    #[test]
    fn test_generate_code_challenge_different_verifiers() {
        let verifier1 = "test-verifier-1";
        let verifier2 = "test-verifier-2";

        let challenge1 = generate_code_challenge(verifier1).unwrap();
        let challenge2 = generate_code_challenge(verifier2).unwrap();

        assert_ne!(challenge1, challenge2);
    }

    #[test]
    fn test_generate_code_challenge_deterministic() {
        let verifier = "same-verifier";

        let challenge1 = generate_code_challenge(verifier).unwrap();
        let challenge2 = generate_code_challenge(verifier).unwrap();

        assert_eq!(challenge1, challenge2);
    }

    #[test]
    fn test_generate_state_length() {
        let state = generate_state();
        // 32 bytes = 64 hex characters
        assert_eq!(state.len(), STATE_LENGTH * 2);
    }

    #[test]
    fn test_generate_state_hex_format() {
        let state = generate_state();
        let valid_hex = "0123456789abcdef";

        for ch in state.chars() {
            assert!(
                valid_hex.contains(ch),
                "Invalid hex character '{}' in state",
                ch
            );
        }
    }

    #[test]
    fn test_generate_state_randomness() {
        let state1 = generate_state();
        let state2 = generate_state();

        assert_ne!(state1, state2);
    }

    #[test]
    fn test_code_verifier_meets_rfc_requirements() {
        // RFC 7636 requires 43-128 characters
        let verifier = generate_code_verifier();
        assert!(verifier.len() >= 43);
        assert!(verifier.len() <= 128);
    }

    #[test]
    fn test_code_challenge_is_base64_url_safe() {
        let verifier = generate_code_verifier();
        let challenge = generate_code_challenge(&verifier).unwrap();

        // Base64-url should not contain +, /, or =
        assert!(!challenge.contains('+'));
        assert!(!challenge.contains('/'));
        assert!(!challenge.contains('='));
    }
}
