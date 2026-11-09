//! The CLI's half of the authorization code flow: the request the consent page gets, and reading what the browser
//! brought back.

use api_types::oauth::{AuthorizationRequestDto, CLI_CLIENT_ID};

use super::loopback::{Callback, LoopbackError};
use super::pkce::Pkce;

#[derive(Debug, thiserror::Error)]
pub enum LoginError {
    #[error(transparent)]
    Loopback(#[from] LoopbackError),
    #[error("can't make a random value for the sign-in")]
    Random(#[source] getrandom::Error),
    #[error("you cancelled the sign-in in the browser")]
    Cancelled,
    #[error("the server refused the sign-in: {error}")]
    Refused { error: String },
    #[error("the browser came back without a code; try again")]
    NoCode,
}

/// The authorization request for the consent page: the CLI's client, `redirect_uri`, the S256 challenge of
/// `pkce`, and `state`.
pub fn authorization_request(redirect_uri: &str, pkce: &Pkce, state: &str) -> AuthorizationRequestDto {
    AuthorizationRequestDto {
        response_type: "code".to_owned(),
        client_id: CLI_CLIENT_ID.to_owned(),
        redirect_uri: redirect_uri.to_owned(),
        code_challenge: pkce.challenge.clone(),
        code_challenge_method: "S256".to_owned(),
        state: state.to_owned(),
    }
}

/// The code in `callback`. A cancel on the consent page comes back as `error=access_denied`.
pub fn read_callback(callback: Callback) -> Result<String, LoginError> {
    match (callback.error, callback.code) {
        (Some(error), _) if error == "access_denied" => Err(LoginError::Cancelled),
        (Some(error), _) => Err(LoginError::Refused { error }),
        (None, Some(code)) => Ok(code),
        (None, None) => Err(LoginError::NoCode),
    }
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    fn callback(code: Option<&str>, error: Option<&str>) -> Callback {
        Callback {
            code: code.map(str::to_owned),
            error: error.map(str::to_owned),
        }
    }

    #[gtest]
    fn the_code_comes_back() -> googletest::Result<()> {
        verify_that!(read_callback(callback(Some("c0de"), None))?, eq("c0de"))
    }

    #[gtest]
    fn cancel_and_other_errors_say_what_happened() {
        expect_that!(
            read_callback(callback(None, Some("access_denied"))),
            err(matches_pattern!(LoginError::Cancelled))
        );
        expect_that!(
            read_callback(callback(None, Some("server_error"))),
            err(matches_pattern!(LoginError::Refused {
                error: eq("server_error")
            }))
        );
        expect_that!(
            read_callback(callback(None, None)),
            err(matches_pattern!(LoginError::NoCode))
        );
    }

    #[gtest]
    fn the_request_asks_for_a_code_with_an_s256_challenge() {
        let pkce = Pkce::from_verifier("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk".to_owned());

        let request = authorization_request("http://127.0.0.1:5000/callback", &pkce, "s");

        expect_that!(request.response_type, eq("code"));
        expect_that!(request.client_id, eq("upheap-cli"));
        expect_that!(
            request.code_challenge,
            eq("E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM")
        );
        expect_that!(request.code_challenge_method, eq("S256"));
    }
}
