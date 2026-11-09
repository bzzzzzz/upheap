//! The PKCE code verifier and its S256 challenge (RFC 7636), and the `state` value of an authorization request.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};

/// A code verifier and its challenge. The verifier stays with the CLI until it exchanges the code.
pub struct Pkce {
    pub verifier: String,
    pub challenge: String,
}

impl Pkce {
    /// A new verifier of 32 random bytes, base64url-encoded to 43 characters, as RFC 7636 §4.1 recommends.
    pub fn generate() -> Result<Self, getrandom::Error> {
        Ok(Self::from_verifier(random_text()?))
    }

    /// The pair for `verifier`.
    pub fn from_verifier(verifier: String) -> Self {
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        Self { verifier, challenge }
    }
}

/// A new `state` value: 32 random bytes, base64url-encoded, which nobody else can guess.
pub fn generate_state() -> Result<String, getrandom::Error> {
    random_text()
}

fn random_text() -> Result<String, getrandom::Error> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    #[gtest]
    fn the_challenge_is_the_s256_of_the_verifier() {
        // RFC 7636 Appendix B.
        let pkce = Pkce::from_verifier("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk".to_owned());
        expect_that!(pkce.challenge, eq("E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"));
    }

    #[gtest]
    fn a_verifier_is_43_unreserved_characters_and_new_each_time() -> googletest::Result<()> {
        let first = Pkce::generate()?;
        let second = Pkce::generate()?;
        verify_that!(first.verifier.len(), eq(43))?;
        verify_that!(
            first
                .verifier
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            eq(true)
        )?;
        verify_that!(first.verifier, not(eq(&second.verifier)))
    }
}
