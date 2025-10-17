/// PKCE (Proof Key for Code Exchange) utilities for OAuth security
///
/// This module wraps the oauth2 crate's PKCE implementation to provide
/// a simplified interface matching our previous custom implementation.
use oauth2::{CsrfToken, PkceCodeChallenge, PkceCodeVerifier};

/// Generates a random code verifier for PKCE
///
/// Uses the oauth2 crate's implementation which generates a
/// cryptographically random string compliant with RFC 7636.
///
/// # Returns
///
/// A random string suitable for use as a PKCE code verifier
pub fn generate_code_verifier() -> String {
    let (_, verifier) = PkceCodeChallenge::new_random_sha256();
    verifier.secret().to_string()
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
/// Returns error if the verifier is invalid (should never happen with valid
/// input)
pub fn generate_code_challenge(verifier: &str) -> anyhow::Result<String> {
    let pkce_verifier = PkceCodeVerifier::new(verifier.to_string());
    let challenge = PkceCodeChallenge::from_code_verifier_sha256(&pkce_verifier);
    Ok(challenge.as_str().to_string())
}

/// Generates a random state parameter for CSRF protection
///
/// Uses the oauth2 crate's CsrfToken which provides
/// cryptographically secure random state generation.
///
/// # Returns
///
/// A random string suitable for use as a CSRF state parameter
pub fn generate_state() -> String {
    CsrfToken::new_random().secret().to_string()
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_generate_code_verifier_length() {
        let verifier = generate_code_verifier();
        // RFC 7636 requires 43-128 characters
        assert!(verifier.len() >= 43);
        assert!(verifier.len() <= 128);
    }

    #[test]
    fn test_generate_code_verifier_charset() {
        let verifier = generate_code_verifier();
        // RFC 7636 allows [A-Z], [a-z], [0-9], "-", ".", "_", "~"
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
        // Use RFC 7636 compliant verifiers (43+ characters)
        let verifier1 = "test-verifier-1-with-sufficient-length-for-rfc-compliance";
        let verifier2 = "test-verifier-2-with-sufficient-length-for-rfc-compliance";

        let challenge1 = generate_code_challenge(verifier1).unwrap();
        let challenge2 = generate_code_challenge(verifier2).unwrap();

        assert_ne!(challenge1, challenge2);
    }

    #[test]
    fn test_generate_code_challenge_deterministic() {
        // Use RFC 7636 compliant verifier (43+ characters)
        let verifier = "same-verifier-with-sufficient-length-for-rfc-compliance";

        let challenge1 = generate_code_challenge(verifier).unwrap();
        let challenge2 = generate_code_challenge(verifier).unwrap();

        assert_eq!(challenge1, challenge2);
    }

    #[test]
    fn test_generate_state_randomness() {
        let state1 = generate_state();
        let state2 = generate_state();

        assert_ne!(state1, state2);
    }

    #[test]
    fn test_generate_state_not_empty() {
        let state = generate_state();
        assert!(!state.is_empty());
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
