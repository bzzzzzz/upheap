//! OAuth 2.0 for first-party non-web clients: the authorization code flow with PKCE (RFC 6749, RFC 7636), refresh
//! token rotation and revocation (RFC 7009). The rules are in `domain::oauth`; this runs them against storage.
//!
//! Codes and tokens are random 256-bit values, made and hashed as session tokens are: the client holds the value,
//! the database only its SHA-256.
//!
//! Every token belongs to the family of the sign-in it came from. A refresh token is good once: using it issues the
//! next pair in the same family. Presenting one that was already used means someone holds a copy, so the whole family
//! is revoked, the legitimate client included, which then has to sign in again (OAuth 2.0 Security BCP §4.14.2).

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use domain::oauth::{
    ACCESS_TOKEN_LIFETIME, AUTHORIZATION_CODE_LIFETIME, AuthorizationError, AuthorizationParams, AuthorizationRequest,
    DeviceName, GrantError, REFRESH_TOKEN_LIFETIME, TokenKind, find_client, is_code_verifier,
};
use domain::{Duration, Timestamp, TokenFamilyId, ValidationError};
use sha2::{Digest, Sha256};
use store::{DatabaseTransaction, StoreError, TransactionTrait};

use crate::auth::{random_token, sha256_hex};
use crate::{Principal, ServiceError, Services};

/// An authorization code, access token or refresh token, as the client holds it. Never log it.
#[derive(Clone, PartialEq, Eq)]
pub struct OAuthSecret(String);

impl OAuthSecret {
    pub fn from_client(raw: String) -> Self {
        Self(raw)
    }

    pub fn expose(&self) -> &str {
        &self.0
    }

    fn generate() -> Self {
        Self(random_token())
    }

    fn to_hash(&self) -> String {
        sha256_hex(&self.0)
    }
}

impl std::fmt::Debug for OAuthSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OAuthSecret(<redacted>)")
    }
}

/// An approved authorization request: where to send the browser, with what.
#[derive(Debug)]
pub struct Approval {
    pub request: AuthorizationRequest,
    pub code: OAuthSecret,
}

/// What a token request for `grant_type=authorization_code` carries.
pub struct CodeExchange<'a> {
    pub client_id: &'a str,
    pub code: &'a OAuthSecret,
    pub redirect_uri: &'a str,
    /// Secret like the code: with it, a leaked code can be exchanged. Left out of `Debug`.
    pub code_verifier: &'a str,
    /// What the client calls the machine, kept on the sign-in for the user's list of them.
    pub device_name: Option<&'a DeviceName>,
}

impl std::fmt::Debug for CodeExchange<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CodeExchange")
            .field("client_id", &self.client_id)
            .field("code", &self.code)
            .field("redirect_uri", &self.redirect_uri)
            .field("code_verifier", &"<redacted>")
            .field("device_name", &self.device_name)
            .finish()
    }
}

/// One of the user's live sign-ins by a client.
#[derive(Debug)]
pub struct ClientSignIn {
    /// The token family's id.
    pub id: TokenFamilyId,
    pub client_id: &'static str,
    /// What the client is called, such as "Upheap CLI".
    pub client_name: &'static str,
    pub device_name: Option<String>,
    pub signed_in_at: Timestamp,
    pub last_used_at: Timestamp,
}

/// A new access token and the refresh token to get the next one with.
#[derive(Debug)]
pub struct TokenPair {
    pub access_token: OAuthSecret,
    pub refresh_token: OAuthSecret,
    /// How long the access token lasts from now.
    pub expires_in: Duration,
}

impl Services {
    /// Checks an authorization request without storing anything, so the consent page knows the client and that its
    /// redirect URI may be used before it shows anything.
    pub fn check_authorization(&self, params: AuthorizationParams<'_>) -> Result<AuthorizationRequest, ServiceError> {
        Ok(AuthorizationRequest::check(params)?)
    }

    /// Approves an authorization request for the signed-in user: stores a single-use code, good for
    /// [`AUTHORIZATION_CODE_LIFETIME`], bound to the client, the redirect URI and the PKCE challenge.
    pub async fn approve_authorization(
        &self,
        principal: Principal,
        params: AuthorizationParams<'_>,
    ) -> Result<Approval, ServiceError> {
        let request = AuthorizationRequest::check(params)?;
        let now = self.now();
        let code = OAuthSecret::generate();
        store::oauth::insert_auth_code(
            &self.db,
            &store::oauth::NewAuthCode {
                code_hash: &code.to_hash(),
                user_id: principal.user_id,
                client_id: request.client.id,
                redirect_uri: request.redirect_uri.as_str(),
                code_challenge: &request.code_challenge,
                created_at: now,
                expires_at: later(now, AUTHORIZATION_CODE_LIFETIME)?,
            },
        )
        .await?;
        tracing::info!(user_id = %principal.user_id, client_id = request.client.id, "authorization approved");
        Ok(Approval { request, code })
    }

    /// Exchanges an authorization code for the first token pair of a new family.
    ///
    /// Fails with [`GrantError`] for a code that is unknown, used, expired, or issued for another client or redirect
    /// URI, and for a verifier that doesn't meet its challenge. A code presented a second time within its lifetime
    /// also revokes the tokens the first exchange issued, as RFC 6749 §4.1.2 asks: one of the two parties presenting
    /// it isn't the client. Past its lifetime it's only refused, so a code left in a browser's history can't end the
    /// sign-in it started at any later time.
    pub async fn exchange_authorization_code(&self, exchange: CodeExchange<'_>) -> Result<TokenPair, ServiceError> {
        require_client(exchange.client_id)?;
        let now = self.now();
        let code_hash = exchange.code.to_hash();
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let code = store::oauth::find_auth_code(&txn, &code_hash)
            .await?
            .ok_or(GrantError::CodeUnknown)?;
        if let Some((_, family_id)) = code.used {
            txn.rollback().await.map_err(StoreError::from)?;
            return self.refuse_reused_code(family_id, code.expires_at, now).await;
        }
        if code.expires_at <= now {
            return Err(GrantError::CodeExpired.into());
        }
        if code.client_id != exchange.client_id {
            return Err(GrantError::ClientMismatch.into());
        }
        if code.redirect_uri != exchange.redirect_uri {
            return Err(GrantError::RedirectUriMismatch.into());
        }
        let verified = is_code_verifier(exchange.code_verifier)
            && code.code_challenge.is_met_by(&s256_challenge(exchange.code_verifier));
        if !verified {
            return Err(GrantError::VerifierMismatch.into());
        }
        let family_id = TokenFamilyId::from_uuid(self.new_id(now));
        store::oauth::insert_token_family(
            &txn,
            family_id,
            code.user_id,
            exchange.client_id,
            exchange.device_name,
            now,
        )
        .await?;
        if !store::oauth::use_auth_code(&txn, &code_hash, now, family_id).await? {
            // Another exchange of this code got there first. It's read again outside this transaction: on MySQL a
            // read inside it would still see the snapshot from before the other exchange committed.
            txn.rollback().await.map_err(StoreError::from)?;
            let winner = store::oauth::find_auth_code(&self.db, &code_hash)
                .await?
                .and_then(|c| c.used);
            return match winner {
                Some((_, family_id)) => self.refuse_reused_code(family_id, code.expires_at, now).await,
                None => Err(GrantError::CodeUsed.into()),
            };
        }
        let pair = issue_pair(&txn, family_id, now).await?;
        txn.commit().await.map_err(StoreError::from)?;
        tracing::info!(user_id = %code.user_id, %family_id, client_id = exchange.client_id, "authorization code exchanged");
        Ok(pair)
    }

    /// Uses a refresh token up and issues the next pair in its family.
    ///
    /// Fails with [`GrantError`] for a refresh token that is unknown, expired, revoked or another client's. One that
    /// was used already revokes its whole family.
    pub async fn refresh_tokens(
        &self,
        client_id: &str,
        refresh_token: &OAuthSecret,
    ) -> Result<TokenPair, ServiceError> {
        require_client(client_id)?;
        let now = self.now();
        let token_hash = refresh_token.to_hash();
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let token = store::oauth::find_token(&txn, &token_hash)
            .await?
            .filter(|token| token.kind == TokenKind::Refresh)
            .ok_or(GrantError::RefreshTokenUnknown)?;
        if token.family.client_id != client_id {
            return Err(GrantError::ClientMismatch.into());
        }
        // The family's row lock, held until commit, puts this refresh and a revocation of the family one after the
        // other: either the revocation waits and ends the pair issued here too, or it went first and is seen here.
        let family = store::oauth::lock_token_family(&txn, token.family.id)
            .await?
            .ok_or(GrantError::RefreshTokenUnknown)?;
        if family.revoked_at.is_some() {
            return Err(GrantError::RefreshTokenRevoked.into());
        }
        if token.used_at.is_some() {
            return revoke_after_reuse(txn, family.id, now, GrantError::RefreshTokenUsed).await;
        }
        if token.expires_at <= now {
            return Err(GrantError::RefreshTokenExpired.into());
        }
        if !store::oauth::use_refresh_token(&txn, &token_hash, now).await? {
            // Another refresh with this token got there first.
            return revoke_after_reuse(txn, family.id, now, GrantError::RefreshTokenUsed).await;
        }
        let pair = issue_pair(&txn, family.id, now).await?;
        txn.commit().await.map_err(StoreError::from)?;
        tracing::info!(user_id = %family.user_id, family_id = %family.id, "tokens refreshed");
        Ok(pair)
    }

    /// Revokes the family of an access or refresh token (RFC 7009), which ends the client's sign-in. A token this
    /// server doesn't know, or one of another client than `client_id`, is left alone and the call still succeeds,
    /// so the answer says nothing about which tokens exist. An unknown `client_id` fails with
    /// [`AuthorizationError::ClientUnknown`].
    pub async fn revoke_token(&self, client_id: Option<&str>, token: &OAuthSecret) -> Result<(), ServiceError> {
        if let Some(client_id) = client_id {
            require_client(client_id)?;
        }
        let Some(found) = store::oauth::find_token(&self.db, &token.to_hash()).await? else {
            return Ok(());
        };
        let family = found.family;
        if client_id.is_some_and(|client_id| client_id != family.client_id) {
            return Ok(());
        }
        let revoked = store::oauth::revoke_token_family(&self.db, family.id, self.now()).await?;
        tracing::info!(user_id = %family.user_id, family_id = %family.id, revoked, "token family revoked");
        Ok(())
    }

    /// The caller's live sign-ins by clients, the most recently used first: not revoked, and with a refresh token that
    /// is unused and unexpired. A sign-in of a client no longer registered is left out.
    pub async fn list_sign_ins(&self, principal: Principal) -> Result<Vec<ClientSignIn>, ServiceError> {
        let families = store::oauth::list_live_token_families(&self.db, principal.user_id, self.now()).await?;
        Ok(families
            .into_iter()
            .filter_map(|family| {
                let client = find_client(&family.client_id)?;
                Some(ClientSignIn {
                    id: family.id,
                    client_id: client.id,
                    client_name: client.display_name,
                    device_name: family.device_name,
                    signed_in_at: family.created_at,
                    last_used_at: family.last_used_at,
                })
            })
            .collect())
    }

    /// Ends one of the caller's sign-ins by revoking its token family, as [`Services::revoke_token`] does. Succeeds
    /// when it was ended already. Fails with [`ServiceError::NotFound`] for a family that doesn't exist or isn't the
    /// caller's, which the answer doesn't tell apart.
    pub async fn end_sign_in(&self, principal: Principal, family_id: TokenFamilyId) -> Result<(), ServiceError> {
        let family = store::oauth::find_token_family(&self.db, family_id)
            .await?
            .filter(|family| family.user_id == principal.user_id)
            .ok_or(ServiceError::NotFound)?;
        let revoked = store::oauth::revoke_token_family(&self.db, family.id, self.now()).await?;
        tracing::info!(user_id = %principal.user_id, family_id = %family.id, revoked, "sign-in ended");
        Ok(())
    }

    /// Resolves an access token to its user, as [`Services::authenticate_session`] does a session. Fails with
    /// [`ServiceError::Unauthenticated`] when the token is unknown, expired or revoked, or is a refresh token.
    pub async fn authenticate_access_token(&self, token: &OAuthSecret) -> Result<Principal, ServiceError> {
        let user_id = store::oauth::find_access_token_user(&self.db, &token.to_hash(), self.now())
            .await?
            .ok_or(ServiceError::Unauthenticated)?;
        Ok(Principal { user_id })
    }
}

impl Services {
    /// Refuses a code presented again, and revokes the family its first exchange issued while the code is within its
    /// lifetime.
    async fn refuse_reused_code(
        &self,
        family_id: TokenFamilyId,
        expires_at: Timestamp,
        now: Timestamp,
    ) -> Result<TokenPair, ServiceError> {
        if expires_at > now {
            let revoked = store::oauth::revoke_token_family(&self.db, family_id, now).await?;
            tracing::warn!(%family_id, revoked, "reused authorization code revoked its token family");
        }
        Err(GrantError::CodeUsed.into())
    }
}

fn require_client(client_id: &str) -> Result<(), ServiceError> {
    find_client(client_id)
        .map(|_| ())
        .ok_or(AuthorizationError::ClientUnknown.into())
}

/// Revokes the family a reused refresh token belongs to, commits that, and fails with `error`.
async fn revoke_after_reuse(
    txn: DatabaseTransaction,
    family_id: TokenFamilyId,
    now: Timestamp,
    error: GrantError,
) -> Result<TokenPair, ServiceError> {
    let revoked = store::oauth::revoke_token_family(&txn, family_id, now).await?;
    txn.commit().await.map_err(StoreError::from)?;
    tracing::warn!(%family_id, revoked, reason = %error, "reused grant revoked its token family");
    Err(error.into())
}

async fn issue_pair(
    txn: &DatabaseTransaction,
    family_id: TokenFamilyId,
    now: Timestamp,
) -> Result<TokenPair, ServiceError> {
    let access_token = OAuthSecret::generate();
    let refresh_token = OAuthSecret::generate();
    for (token, kind, lifetime) in [
        (&access_token, TokenKind::Access, ACCESS_TOKEN_LIFETIME),
        (&refresh_token, TokenKind::Refresh, REFRESH_TOKEN_LIFETIME),
    ] {
        let token = store::oauth::NewToken {
            token_hash: &token.to_hash(),
            family_id,
            kind,
            created_at: now,
            expires_at: later(now, lifetime)?,
        };
        store::oauth::insert_token(txn, &token).await?;
    }
    Ok(TokenPair {
        access_token,
        refresh_token,
        expires_in: ACCESS_TOKEN_LIFETIME,
    })
}

fn later(now: Timestamp, lifetime: Duration) -> Result<Timestamp, ServiceError> {
    now.checked_add(lifetime)
        .ok_or(ServiceError::Invalid(ValidationError::TimestampOutOfRange))
}

/// The S256 code challenge of `verifier`: its SHA-256 in base64url without padding (RFC 7636 §4.2).
fn s256_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

#[cfg(test)]
mod tests {
    use domain::oauth::CodeChallenge;
    use googletest::prelude::*;

    use super::*;

    #[gtest]
    fn the_s256_challenge_is_the_one_rfc_7636_gives() {
        // RFC 7636, Appendix B.
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";

        expect_that!(
            s256_challenge(verifier),
            eq("E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM")
        );
    }

    #[gtest]
    fn a_verifier_meets_only_its_own_challenge() -> googletest::Result<()> {
        let challenge = CodeChallenge::parse("E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM", "S256").or_fail()?;

        expect_that!(
            challenge.is_met_by(&s256_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk")),
            eq(true)
        );
        expect_that!(
            challenge.is_met_by(&s256_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXl")),
            eq(false)
        );
        Ok(())
    }

    #[gtest]
    fn secrets_are_redacted_in_debug_output() {
        let secret = OAuthSecret::from_client("very secret".to_owned());
        let exchange = CodeExchange {
            client_id: "upheap-cli",
            code: &secret,
            redirect_uri: "http://127.0.0.1:1/",
            code_verifier: "secret verifier",
            device_name: None,
        };

        expect_that!(format!("{secret:?}"), not(contains_substring("very secret")));
        expect_that!(format!("{exchange:?}"), not(contains_substring("secret verifier")));
        expect_that!(format!("{exchange:?}"), not(contains_substring("very secret")));
    }
}
