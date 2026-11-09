//! OAuth 2.0 for first-party non-web clients: the registered clients, their loopback redirect URIs, PKCE values and
//! the lifetimes of codes and tokens. Hashing and storage are the service's; this is only the rules.
//!
//! Only registered first-party public clients may ask for tokens, and the registry is a constant rather than a table:
//! there is one client today, the `upheap` CLI, and adding another is a code change that gets reviewed.

use crate::Duration;

/// How long an authorization code can be exchanged for tokens. RFC 6749 §4.1.2 recommends at most ten minutes.
pub const AUTHORIZATION_CODE_LIFETIME: Duration = Duration::from_minutes(10);
/// How long an access token is accepted.
pub const ACCESS_TOKEN_LIFETIME: Duration = Duration::from_minutes(15);
/// How long a refresh token can be used, counted from when it was issued. Each refresh issues a new one, so a client
/// in use stays signed in and one left alone for this long has to sign in again.
pub const REFRESH_TOKEN_LIFETIME: Duration = Duration::from_days(30);

/// What a token is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenKind {
    /// Sent as `Authorization: Bearer` on API requests, until it expires.
    Access,
    /// Exchanged once for the next access and refresh tokens.
    Refresh,
}

/// A registered client.
#[derive(Debug, PartialEq, Eq)]
pub struct Client {
    pub id: &'static str,
    /// What the consent page calls the client.
    pub display_name: &'static str,
}

/// The `upheap` command-line client (design/work/cli.md).
pub const CLI_CLIENT: Client = Client {
    id: "upheap-cli",
    display_name: "Upheap CLI",
};

/// Every registered client. All are public clients with loopback redirect URIs (RFC 8252 §7.3).
const CLIENTS: &[Client] = &[CLI_CLIENT];

/// The registered client with this `client_id`.
pub fn find_client(client_id: &str) -> Option<&'static Client> {
    CLIENTS.iter().find(|client| client.id == client_id)
}

/// Why an authorization request was refused before anything was stored.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum AuthorizationError {
    #[error("response_type must be code")]
    ResponseTypeUnsupported,
    #[error("this app isn't a client this server knows")]
    ClientUnknown,
    #[error("redirect_uri must be http://127.0.0.1 or http://[::1] with a port, and a path at most")]
    RedirectUriInvalid,
    #[error("code_challenge must be the 43-character S256 challenge of a PKCE code verifier")]
    CodeChallengeInvalid,
    #[error("code_challenge_method must be S256")]
    CodeChallengeMethodUnsupported,
    #[error("state must not be empty")]
    StateMissing,
}

/// Why a token request's grant was refused: RFC 6749's `invalid_grant`, whatever the reason. The reason is for the
/// response's description and the log; clients branch on the RFC code.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum GrantError {
    #[error("the authorization code isn't one this server issued")]
    CodeUnknown,
    #[error("the authorization code was used already")]
    CodeUsed,
    #[error("the authorization code has expired")]
    CodeExpired,
    #[error("the authorization code or refresh token was issued to another client")]
    ClientMismatch,
    #[error("redirect_uri isn't the one the authorization code was issued for")]
    RedirectUriMismatch,
    #[error("code_verifier doesn't match the code challenge")]
    VerifierMismatch,
    #[error("the refresh token isn't one this server issued")]
    RefreshTokenUnknown,
    #[error("the refresh token was used already, so every token of its sign-in is revoked")]
    RefreshTokenUsed,
    #[error("the refresh token has expired")]
    RefreshTokenExpired,
    #[error("the refresh token has been revoked")]
    RefreshTokenRevoked,
}

/// A redirect URI a registered client may use: `http://127.0.0.1:<port><path>` or `http://[::1]:<port><path>`.
///
/// Native clients listen on a loopback port they pick when they start, so any port is accepted (RFC 8252 §7.3). The
/// rest is as strict as can be: no `localhost` (it can resolve elsewhere, §8.3), no other scheme or host, no user
/// info, no query and no fragment, so appending `?code=…&state=…` always gives the URI that was approved. The path
/// takes only the characters RFC 3986 allows in one, so nothing in it can be read as another part of the URI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RedirectUri(String);

impl RedirectUri {
    /// Longest redirect URI accepted. Loopback ones are short; this only bounds what is stored.
    pub const MAX_LEN: usize = 512;

    pub fn parse(raw: &str) -> Result<Self, AuthorizationError> {
        let invalid = AuthorizationError::RedirectUriInvalid;
        if raw.len() > Self::MAX_LEN {
            return Err(invalid);
        }
        let rest = raw
            .strip_prefix("http://127.0.0.1:")
            .or_else(|| raw.strip_prefix("http://[::1]:"))
            .ok_or(invalid.clone())?;
        let (port, path) = rest.find('/').map_or((rest, ""), |slash| rest.split_at(slash));
        if !is_port(port) || !is_path(path) {
            return Err(invalid);
        }
        Ok(Self(raw.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 1 to 65535, in digits only.
fn is_port(port: &str) -> bool {
    !port.is_empty()
        && port.len() <= 5
        && port.bytes().all(|b| b.is_ascii_digit())
        && port.parse::<u16>().is_ok_and(|p| p > 0)
}

/// An RFC 3986 path: segments of unreserved characters, sub-delimiters, `:`, `@` and percent-escapes, after a `/`.
/// Empty is fine too.
fn is_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            let escape = bytes.get(i + 1..i + 3);
            if !escape.is_some_and(|hex| hex.iter().all(u8::is_ascii_hexdigit)) {
                return false;
            }
            i += 3;
            continue;
        }
        if !(b.is_ascii_alphanumeric() || b"-._~!$&'()*+,;=:@/".contains(&b)) {
            return false;
        }
        i += 1;
    }
    true
}

/// A PKCE code challenge made with S256: the base64url SHA-256 of the client's code verifier, without padding, so
/// always 43 characters (RFC 7636 §4.2). `plain` isn't accepted, since it gives no protection if the request leaks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeChallenge(String);

impl CodeChallenge {
    /// Only `S256` is accepted.
    pub fn parse(challenge: &str, method: &str) -> Result<Self, AuthorizationError> {
        if method != "S256" {
            return Err(AuthorizationError::CodeChallengeMethodUnsupported);
        }
        let valid = challenge.len() == 43 && challenge.bytes().all(is_base64url);
        valid
            .then(|| Self(challenge.to_owned()))
            .ok_or(AuthorizationError::CodeChallengeInvalid)
    }

    /// A challenge read back from storage, where only parsed ones are written.
    pub fn from_stored(challenge: String) -> Self {
        Self(challenge)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether `verifier_digest`, the base64url SHA-256 of the verifier the client presented, is this challenge.
    /// The hashing is the caller's, so this crate needs no crypto.
    pub fn is_met_by(&self, verifier_digest: &str) -> bool {
        self.0 == verifier_digest
    }
}

fn is_base64url(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'_'
}

/// Whether `verifier` is a well-formed PKCE code verifier: 43 to 128 unreserved characters (RFC 7636 §4.1). One that
/// isn't can't match any challenge a conforming client made, so the exchange fails as for a wrong one.
pub fn is_code_verifier(verifier: &str) -> bool {
    (43..=128).contains(&verifier.len())
        && verifier
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~'))
}

/// An authorization request (RFC 6749 §4.1.1 with RFC 7636's PKCE) that names a registered client and one of its
/// redirect URIs, with an S256 challenge. Only a checked request can be approved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorizationRequest {
    pub client: &'static Client,
    pub redirect_uri: RedirectUri,
    pub code_challenge: CodeChallenge,
    /// The client's value, sent back with the code so it can tell its own request's answer from a forged one.
    pub state: String,
}

/// An authorization request as the client sent it, every value as text.
#[derive(Clone, Copy, Debug)]
pub struct AuthorizationParams<'a> {
    pub response_type: &'a str,
    pub client_id: &'a str,
    pub redirect_uri: &'a str,
    pub code_challenge: &'a str,
    pub code_challenge_method: &'a str,
    pub state: &'a str,
}

impl AuthorizationRequest {
    /// Checks the request, client and redirect URI first: until both are known good, the error can't be sent to the
    /// redirect URI and is shown to the user instead.
    pub fn check(params: AuthorizationParams<'_>) -> Result<Self, AuthorizationError> {
        let client = find_client(params.client_id).ok_or(AuthorizationError::ClientUnknown)?;
        let redirect_uri = RedirectUri::parse(params.redirect_uri)?;
        if params.response_type != "code" {
            return Err(AuthorizationError::ResponseTypeUnsupported);
        }
        let code_challenge = CodeChallenge::parse(params.code_challenge, params.code_challenge_method)?;
        if params.state.is_empty() {
            return Err(AuthorizationError::StateMissing);
        }
        Ok(Self {
            client,
            redirect_uri,
            code_challenge,
            state: params.state.to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    #[gtest]
    fn loopback_redirect_uris_with_any_port_and_path_are_accepted() {
        for uri in [
            "http://127.0.0.1:1",
            "http://127.0.0.1:53682/",
            "http://127.0.0.1:65535/callback",
            "http://[::1]:8080/oauth/callback",
            "http://127.0.0.1:8080/a%2Fb/c-d_e.f~g:h@i",
        ] {
            expect_that!(
                RedirectUri::parse(uri).map(|u| u.as_str().to_owned()),
                ok(eq(uri)),
                "{uri}"
            );
        }
    }

    #[gtest]
    fn other_redirect_uris_are_refused() {
        for uri in [
            "",
            "http://localhost:8080/callback",
            "https://127.0.0.1:8080/callback",
            "http://127.0.0.1/callback",
            "http://127.0.0.1:/callback",
            "http://127.0.0.1:0/callback",
            "http://127.0.0.1:65536/callback",
            "http://127.0.0.1:080a/callback",
            "http://127.0.0.1:8080@evil.example/callback",
            "http://127.0.0.1.evil.example:8080/callback",
            "http://127.0.0.2:8080/callback",
            "http://[::2]:8080/callback",
            "http://[::1]/callback",
            "http://127.0.0.1:8080/callback?next=x",
            "http://127.0.0.1:8080/callback#fragment",
            "http://127.0.0.1:8080#",
            "http://127.0.0.1:8080/a b",
            "http://127.0.0.1:8080/%zz",
            "http://127.0.0.1:8080/%2",
            "http://127.0.0.1:8080/\\evil",
            "HTTP://127.0.0.1:8080/callback",
            "urn:ietf:wg:oauth:2.0:oob",
        ] {
            expect_that!(
                RedirectUri::parse(uri),
                err(eq(&AuthorizationError::RedirectUriInvalid)),
                "{uri:?}"
            );
        }
    }

    #[gtest]
    fn a_redirect_uri_over_the_limit_is_refused() {
        let uri = format!("http://127.0.0.1:8080/{}", "a".repeat(RedirectUri::MAX_LEN));

        expect_that!(
            RedirectUri::parse(&uri),
            err(eq(&AuthorizationError::RedirectUriInvalid))
        );
    }

    #[gtest]
    fn only_s256_challenges_of_the_right_shape_are_accepted() {
        let good = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";

        expect_that!(
            CodeChallenge::parse(good, "S256").map(|c| c.as_str().to_owned()),
            ok(eq(good))
        );
        expect_that!(
            CodeChallenge::parse(good, "plain"),
            err(eq(&AuthorizationError::CodeChallengeMethodUnsupported))
        );
        expect_that!(
            CodeChallenge::parse(&good[1..], "S256"),
            err(eq(&AuthorizationError::CodeChallengeInvalid))
        );
        expect_that!(
            CodeChallenge::parse("E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw+cM", "S256"),
            err(eq(&AuthorizationError::CodeChallengeInvalid))
        );
    }

    #[gtest]
    fn code_verifiers_are_43_to_128_unreserved_characters() {
        expect_that!(is_code_verifier(&"a".repeat(43)), eq(true));
        expect_that!(is_code_verifier(&"Az09-._~".repeat(16)), eq(true));
        expect_that!(is_code_verifier(&"a".repeat(42)), eq(false));
        expect_that!(is_code_verifier(&"a".repeat(129)), eq(false));
        expect_that!(is_code_verifier(&format!("{}+", "a".repeat(43))), eq(false));
    }

    fn params() -> AuthorizationParams<'static> {
        AuthorizationParams {
            response_type: "code",
            client_id: "upheap-cli",
            redirect_uri: "http://127.0.0.1:8080/callback",
            code_challenge: "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM",
            code_challenge_method: "S256",
            state: "xyz",
        }
    }

    #[gtest]
    fn a_request_from_the_cli_is_accepted() {
        let request = AuthorizationRequest::check(params());

        expect_that!(request.map(|r| r.client.id), ok(eq(&"upheap-cli")));
    }

    #[gtest]
    fn each_bad_part_of_a_request_has_its_own_error() {
        let cases = [
            (
                AuthorizationParams {
                    client_id: "other",
                    ..params()
                },
                AuthorizationError::ClientUnknown,
            ),
            (
                AuthorizationParams {
                    redirect_uri: "https://example.test/",
                    ..params()
                },
                AuthorizationError::RedirectUriInvalid,
            ),
            (
                AuthorizationParams {
                    response_type: "token",
                    ..params()
                },
                AuthorizationError::ResponseTypeUnsupported,
            ),
            (
                AuthorizationParams {
                    code_challenge: "",
                    ..params()
                },
                AuthorizationError::CodeChallengeInvalid,
            ),
            (
                AuthorizationParams {
                    code_challenge_method: "",
                    ..params()
                },
                AuthorizationError::CodeChallengeMethodUnsupported,
            ),
            (
                AuthorizationParams { state: "", ..params() },
                AuthorizationError::StateMissing,
            ),
        ];
        for (params, error) in cases {
            expect_that!(AuthorizationRequest::check(params), err(eq(&error)));
        }
    }

    #[gtest]
    fn the_client_and_redirect_uri_are_checked_before_the_rest() {
        let request = AuthorizationParams {
            client_id: "other",
            redirect_uri: "https://evil.example/",
            response_type: "token",
            ..params()
        };

        expect_that!(
            AuthorizationRequest::check(request),
            err(eq(&AuthorizationError::ClientUnknown))
        );
    }
}
