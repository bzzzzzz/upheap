//! Configuration, read once from environment variables at startup and validated before anything else runs.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, bail};
use service::oidc::OidcSettings;
use service::scim::ScimSettings;

/// Every option the server reads. Documented in the README; validated by [`Config::from_env`].
#[derive(Debug)]
pub struct Config {
    /// `DATABASE_URL`, required: `sqlite://path/to/heap.db?mode=rwc`, `mysql://user:pass@host/db` or
    /// `postgres://user:pass@host/db`.
    pub database_url: String,
    /// `DATABASE_MAX_CONNECTIONS`, default `5`: the most database connections this server keeps open. Small, because
    /// on Cloud Run several instances may share one database's connection limit.
    pub database_max_connections: u32,
    /// `DATABASE_CONNECT_DEADLINE_SECS`, default `45`: how long startup keeps new tries at the database coming before
    /// it gives up. A last try and the migrations still fit in the ~60 s the Cloud Run startup probe allows.
    pub database_connect_deadline: Duration,
    /// `BASE_URL`, required: the public URL users open, for example `https://heap.example.com`.
    /// Session cookies are `Secure` unless it starts with `http://`.
    pub base_url: String,
    /// `BIND_ADDR`, default `0.0.0.0:8080`.
    pub bind_addr: SocketAddr,
    /// `ALLOW_REGISTRATION`, default `true`.
    pub allow_registration: bool,
    /// `UPHEAP_PASSWORD_SIGN_IN`, default `true`. `false` makes the OpenID Connect provider the only way to sign in.
    pub password_sign_in: bool,
    /// `SESSION_TTL_DAYS`, default `30`.
    pub session_ttl_days: u16,
    /// `LOG_FORMAT`: `json` (default) or `pretty`.
    pub log_format: LogFormat,
    /// `UPHEAP_OIDC_ISSUER`, `UPHEAP_OIDC_CLIENT_ID`, `UPHEAP_OIDC_CLIENT_SECRET` and `UPHEAP_OIDC_LABEL`, all or
    /// none, `UPHEAP_OIDC_SCOPES`, default `openid email profile`, `UPHEAP_OIDC_GROUPS_CLAIM`, default `groups`, and
    /// `UPHEAP_OIDC_SUBJECT_CLAIM`, default `sub`, the ID token claim an account is matched on: the provider people
    /// may sign in through, and `UPHEAP_OIDC_PROVIDER_LOGOUT`, default `false`: whether signing out of the web app
    /// also signs out at the provider. No request is made to it at startup.
    pub oidc: Option<OidcSettings>,
    /// `UPHEAP_OIDC_GROUP_TEAMS`: a JSON object from team slug to the provider groups whose members the team has.
    /// Empty when unset. The slugs are checked against the teams once the database is up, not here.
    pub group_teams: Vec<(String, Vec<String>)>,
    /// `UPHEAP_SCIM_TOKEN`: the bearer token the provider's SCIM client sends, at least 32 characters. `None` when unset
    /// or blank: the SCIM endpoints aren't served. Needs the provider.
    pub scim: Option<ScimSettings>,
    /// `UPHEAP_OPERATOR_CONTACT`: one line, at most 200 characters, shown wherever the web app says to ask whoever runs
    /// the server. `None` when unset or blank.
    pub operator_contact: Option<String>,
    /// `MAIL_URL` and `MAIL_FROM`: where mail goes and who it is from. `None` when `MAIL_URL` is unset: mail is off.
    pub mail: Option<Arc<mail::Mailer>>,
}

/// The URL schemes of the databases the server can use. sqlx takes both spellings of Postgres's.
const DATABASE_SCHEMES: [&str; 4] = ["sqlite:", "mysql://", "postgres://", "postgresql://"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogFormat {
    Json,
    Pretty,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> anyhow::Result<Self> {
        let required = |name: &str| {
            lookup(name)
                .filter(|v| !v.trim().is_empty())
                .with_context(|| format!("{name} is required"))
        };
        let database_url = required("DATABASE_URL")?;
        if !DATABASE_SCHEMES.iter().any(|scheme| database_url.starts_with(scheme)) {
            bail!("DATABASE_URL must start with sqlite:, mysql://, postgres:// or postgresql://");
        }
        let database_max_connections = match lookup("DATABASE_MAX_CONNECTIONS") {
            None => 5,
            Some(raw) => raw
                .parse()
                .ok()
                .filter(|n| *n >= 1)
                .context("DATABASE_MAX_CONNECTIONS must be a whole number, at least 1")?,
        };
        let database_connect_deadline = match lookup("DATABASE_CONNECT_DEADLINE_SECS") {
            None => Duration::from_secs(45),
            Some(raw) => Duration::from_secs(
                raw.parse()
                    .context("DATABASE_CONNECT_DEADLINE_SECS must be a whole number of seconds")?,
            ),
        };
        let base_url = required("BASE_URL")?.trim_end_matches('/').to_owned();
        if !(base_url.starts_with("https://") || base_url.starts_with("http://")) {
            bail!("BASE_URL must start with https:// or http://");
        }
        let bind_addr = lookup("BIND_ADDR")
            .unwrap_or_else(|| "0.0.0.0:8080".to_owned())
            .parse()
            .context("BIND_ADDR must be an address such as 0.0.0.0:8080")?;
        let allow_registration = match lookup("ALLOW_REGISTRATION").as_deref() {
            None | Some("true") => true,
            Some("false") => false,
            Some(other) => bail!("ALLOW_REGISTRATION must be true or false, not {other:?}"),
        };
        let password_sign_in = match lookup("UPHEAP_PASSWORD_SIGN_IN").as_deref() {
            None | Some("true") => true,
            Some("false") => false,
            Some(other) => bail!("UPHEAP_PASSWORD_SIGN_IN must be true or false, not {other:?}"),
        };
        let session_ttl_days = match lookup("SESSION_TTL_DAYS") {
            None => 30,
            Some(raw) => raw
                .parse()
                .ok()
                .filter(|d| *d >= 1)
                .context("SESSION_TTL_DAYS must be a whole number of days, at least 1")?,
        };
        let log_format = match lookup("LOG_FORMAT").as_deref() {
            None | Some("json") => LogFormat::Json,
            Some("pretty") => LogFormat::Pretty,
            Some(other) => bail!("LOG_FORMAT must be json or pretty, not {other:?}"),
        };
        let oidc = oidc_settings(&lookup, &base_url)?;
        if !password_sign_in && oidc.is_none() {
            bail!(
                "UPHEAP_PASSWORD_SIGN_IN=false needs an OpenID Connect provider: set the UPHEAP_OIDC_* variables, or nobody can sign in"
            );
        }
        let provider_logout = provider_logout(&lookup, oidc.is_some())?;
        let oidc = oidc.map(|oidc| oidc.with_provider_logout(provider_logout));
        let group_teams = group_teams(&lookup, oidc.is_some())?;
        let scim = scim_settings(&lookup, oidc.is_some(), &base_url)?;
        let operator_contact = operator_contact(&lookup)?;
        let mail = match lookup("MAIL_URL").filter(|v| !v.trim().is_empty()) {
            None => None,
            Some(url) => {
                let from = lookup("MAIL_FROM")
                    .filter(|v| !v.trim().is_empty())
                    .context("MAIL_FROM is required when MAIL_URL is set")?;
                // The errors don't repeat the URL, so none of them holds the SMTP password.
                let mailer = mail::Mailer::new(url.trim(), from.trim()).map_err(|error| match error {
                    mail::MailerError::InvalidSender(_) => anyhow::Error::new(error)
                        .context("MAIL_FROM must be an address such as \"Upheap <upheap@example.com>\""),
                    mail::MailerError::InvalidUrl(_) => anyhow::Error::new(error).context("MAIL_URL"),
                })?;
                Some(Arc::new(mailer))
            }
        };
        Ok(Self {
            database_url,
            database_max_connections,
            database_connect_deadline,
            base_url,
            bind_addr,
            allow_registration,
            password_sign_in,
            session_ttl_days,
            log_format,
            oidc,
            group_teams,
            scim,
            operator_contact,
            mail,
        })
    }

    pub fn cookies_secure(&self) -> bool {
        !self.base_url.starts_with("http://")
    }
}

/// The OpenID Connect provider: none when none of its four required variables is set, and an error naming each one
/// that is missing when only some are. `UPHEAP_OIDC_SCOPES` alone is ignored.
fn oidc_settings(lookup: &impl Fn(&str) -> Option<String>, base_url: &str) -> anyhow::Result<Option<OidcSettings>> {
    const REQUIRED: [&str; 4] = [
        "UPHEAP_OIDC_ISSUER",
        "UPHEAP_OIDC_CLIENT_ID",
        "UPHEAP_OIDC_CLIENT_SECRET",
        "UPHEAP_OIDC_LABEL",
    ];
    let values = REQUIRED.map(|name| lookup(name).filter(|value| !value.trim().is_empty()));
    let [Some(issuer), Some(client_id), Some(client_secret), Some(label)] = &values else {
        if values.iter().all(Option::is_none) {
            return Ok(None);
        }
        let set = REQUIRED[values.iter().position(Option::is_some).unwrap_or_default()];
        let missing: Vec<_> = REQUIRED
            .iter()
            .zip(&values)
            .filter(|(_, value)| value.is_none())
            .map(|(name, _)| format!("{name} is required when {set} is set"))
            .collect();
        bail!("{}", missing.join("; "));
    };
    let scopes = lookup("UPHEAP_OIDC_SCOPES").unwrap_or_else(|| "openid email profile".to_owned());
    // The issuer is kept as given: the ID token's `iss` must equal it byte for byte.
    let settings = OidcSettings::new(issuer, client_id, client_secret, label, &scopes, base_url)
        .context("UPHEAP_OIDC_ISSUER, UPHEAP_OIDC_LABEL or BASE_URL is not usable for the OpenID Connect provider")?
        .with_groups_claim(&lookup("UPHEAP_OIDC_GROUPS_CLAIM").unwrap_or_default())
        .with_subject_claim(&lookup("UPHEAP_OIDC_SUBJECT_CLAIM").unwrap_or_default());
    Ok(Some(settings))
}

/// Whether `UPHEAP_OIDC_PROVIDER_LOGOUT` is on: off when unset or blank, and an error when it is neither `true` nor
/// `false`, or is `true` without a provider.
fn provider_logout(lookup: &impl Fn(&str) -> Option<String>, has_provider: bool) -> anyhow::Result<bool> {
    const NAME: &str = "UPHEAP_OIDC_PROVIDER_LOGOUT";
    match lookup(NAME).as_deref().map(str::trim) {
        None | Some("" | "false") => Ok(false),
        Some("true") if has_provider => Ok(true),
        Some("true") => bail!("{NAME} is set, but there is no OpenID Connect provider to sign out of"),
        Some(other) => bail!("{NAME} must be true or false, not {other:?}"),
    }
}

/// The map from team slug to provider groups: none when `UPHEAP_OIDC_GROUP_TEAMS` is unset or blank, and an error
/// when it is set without a provider, isn't a JSON object of arrays of strings, or gives a team no group (which
/// would take every member out at their next sign-in).
fn group_teams(
    lookup: &impl Fn(&str) -> Option<String>,
    has_provider: bool,
) -> anyhow::Result<Vec<(String, Vec<String>)>> {
    const NAME: &str = "UPHEAP_OIDC_GROUP_TEAMS";
    let Some(raw) = lookup(NAME).filter(|value| !value.trim().is_empty()) else {
        return Ok(Vec::new());
    };
    if !has_provider {
        bail!("{NAME} is set, but there is no OpenID Connect provider to read groups from");
    }
    let shape = || format!("{NAME} must be a JSON object from team slug to a list of provider groups");
    let serde_json::Value::Object(map) = serde_json::from_str(&raw).with_context(shape)? else {
        bail!("{}", shape());
    };
    map.into_iter()
        .map(|(slug, value)| {
            let groups: Vec<String> = serde_json::from_value(value).with_context(shape)?;
            if groups.is_empty() {
                bail!("{NAME} gives the team {slug:?} no group");
            }
            Ok((slug, groups))
        })
        .collect()
}

/// The operator's contact line, trimmed: none when `UPHEAP_OPERATOR_CONTACT` is unset or blank, and an error when it is
/// longer than [`MAX_OPERATOR_CONTACT`] characters.
fn operator_contact(lookup: &impl Fn(&str) -> Option<String>) -> anyhow::Result<Option<String>> {
    const NAME: &str = "UPHEAP_OPERATOR_CONTACT";
    let Some(contact) = lookup(NAME)
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    if contact.chars().count() > MAX_OPERATOR_CONTACT {
        bail!("{NAME} must be at most {MAX_OPERATOR_CONTACT} characters");
    }
    Ok(Some(contact))
}

/// The longest `UPHEAP_OPERATOR_CONTACT`: a line, not a page, on every screen that shows it.
const MAX_OPERATOR_CONTACT: usize = 200;

/// The SCIM access: none when `UPHEAP_SCIM_TOKEN` is unset or blank, and an error when it is set without a provider
/// or is shorter than [`SCIM_TOKEN_MIN_CHARS`]. The token is never in the error.
fn scim_settings(
    lookup: &impl Fn(&str) -> Option<String>,
    has_provider: bool,
    base_url: &str,
) -> anyhow::Result<Option<ScimSettings>> {
    const NAME: &str = "UPHEAP_SCIM_TOKEN";
    let Some(token) = lookup(NAME)
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    if !has_provider {
        bail!("{NAME} is set, but there is no OpenID Connect provider whose people it could provision");
    }
    if token.chars().count() < SCIM_TOKEN_MIN_CHARS {
        bail!("{NAME} must be at least {SCIM_TOKEN_MIN_CHARS} characters");
    }
    Ok(Some(ScimSettings::new(&token, base_url)))
}

/// The shortest `UPHEAP_SCIM_TOKEN`: `openssl rand -hex 32` makes 64.
const SCIM_TOKEN_MIN_CHARS: usize = 32;

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn config(vars: &[(&str, &str)]) -> anyhow::Result<Config> {
        let vars: HashMap<String, String> = vars.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect();
        Config::from_lookup(|name| vars.get(name).cloned())
    }

    #[test]
    fn requires_database_and_base_url() {
        assert!(config(&[("BASE_URL", "https://x")]).is_err());
        assert!(config(&[("DATABASE_URL", "sqlite://x.db")]).is_err());
    }

    #[test]
    fn defaults_are_secure() -> anyhow::Result<()> {
        let config = config(&[
            ("DATABASE_URL", "sqlite://x.db"),
            ("BASE_URL", "https://heap.example.com/"),
        ])?;
        assert!(config.cookies_secure());
        assert_eq!(config.base_url, "https://heap.example.com");
        assert_eq!(config.log_format, LogFormat::Json);
        Ok(())
    }

    #[test]
    fn rejects_unknown_values() {
        let base = [("DATABASE_URL", "sqlite://x.db"), ("BASE_URL", "https://x")];
        assert!(config(&[base[0], base[1], ("ALLOW_REGISTRATION", "yes")]).is_err());
        assert!(config(&[base[0], base[1], ("DATABASE_URL", "mssql://x")]).is_err());
        assert!(config(&[base[0], base[1], ("DATABASE_MAX_CONNECTIONS", "0")]).is_err());
        assert!(config(&[base[0], base[1], ("DATABASE_MAX_CONNECTIONS", "five")]).is_err());
        assert!(config(&[base[0], base[1], ("DATABASE_CONNECT_DEADLINE_SECS", "1m")]).is_err());
    }

    #[test]
    fn sets_the_pool_size() -> anyhow::Result<()> {
        let base = [("DATABASE_URL", "sqlite://x.db"), ("BASE_URL", "https://x")];
        assert_eq!(config(&base)?.database_max_connections, 5);
        let config = config(&[base[0], base[1], ("DATABASE_MAX_CONNECTIONS", "20")])?;
        assert_eq!(config.database_max_connections, 20);
        Ok(())
    }

    #[test]
    fn sets_the_connect_deadline() -> anyhow::Result<()> {
        let base = [("DATABASE_URL", "sqlite://x.db"), ("BASE_URL", "https://x")];
        assert_eq!(config(&base)?.database_connect_deadline, Duration::from_secs(45));
        let config = config(&[base[0], base[1], ("DATABASE_CONNECT_DEADLINE_SECS", "0")])?;
        assert_eq!(config.database_connect_deadline, Duration::ZERO);
        Ok(())
    }

    #[test]
    fn mail_is_off_without_settings_and_on_with_both() -> anyhow::Result<()> {
        let base = [("DATABASE_URL", "sqlite://x.db"), ("BASE_URL", "https://x")];
        assert!(config(&base)?.mail.is_none());
        assert!(
            config(&[base[0], base[1], ("MAIL_FROM", "u@example.com")])?
                .mail
                .is_none()
        );
        let on = config(&[
            base[0],
            base[1],
            ("MAIL_URL", "log:"),
            ("MAIL_FROM", "Upheap <u@example.com>"),
        ])?;
        assert_eq!(on.mail.map(|m| m.describe().to_owned()).as_deref(), Some("log:"));
        Ok(())
    }

    #[test]
    fn mail_url_needs_a_sender_and_a_bad_one_doesnt_leak_the_password() {
        let base = [("DATABASE_URL", "sqlite://x.db"), ("BASE_URL", "https://x")];
        assert!(config(&[base[0], base[1], ("MAIL_URL", "log:")]).is_err());
        let error = config(&[
            base[0],
            base[1],
            ("MAIL_URL", "ftp://u:s3cret@smtp.example.com"),
            ("MAIL_FROM", "u@example.com"),
        ])
        .expect_err("an unknown scheme is rejected");
        assert!(!format!("{error:#}").contains("s3cret"));
    }

    #[test]
    fn accepts_postgres() -> anyhow::Result<()> {
        for url in [
            "postgres://u:p@db.example.com/heap",
            "postgresql://u:p@db.example.com/heap",
        ] {
            let config = config(&[("DATABASE_URL", url), ("BASE_URL", "https://x")])?;
            assert_eq!(config.database_url, url);
            assert_eq!(config.database_max_connections, 5);
        }
        Ok(())
    }

    const BASE: [(&str, &str); 2] = [("DATABASE_URL", "sqlite://x.db"), ("BASE_URL", "https://x")];
    const OIDC: [(&str, &str); 4] = [
        ("UPHEAP_OIDC_ISSUER", "https://idp.example.com/"),
        ("UPHEAP_OIDC_CLIENT_ID", "upheap"),
        ("UPHEAP_OIDC_CLIENT_SECRET", "s3cret"),
        ("UPHEAP_OIDC_LABEL", " Acme SSO "),
    ];

    #[test]
    fn has_no_provider_unless_one_is_set() -> anyhow::Result<()> {
        assert!(config(&BASE)?.oidc.is_none());
        let scopes_only = [BASE[0], BASE[1], ("UPHEAP_OIDC_SCOPES", "openid email")];
        assert!(config(&scopes_only)?.oidc.is_none());
        Ok(())
    }

    #[test]
    fn reads_whether_passwords_sign_in() -> anyhow::Result<()> {
        let with_provider: Vec<_> = BASE.iter().chain(&OIDC).copied().collect();
        assert!(config(&BASE)?.password_sign_in);
        for (value, expected) in [("true", true), ("false", false)] {
            let vars: Vec<_> = with_provider
                .iter()
                .copied()
                .chain([("UPHEAP_PASSWORD_SIGN_IN", value)])
                .collect();
            assert_eq!(config(&vars)?.password_sign_in, expected);
        }
        let other = config(&[BASE[0], BASE[1], ("UPHEAP_PASSWORD_SIGN_IN", "no")]).unwrap_err();
        assert!(format!("{other:#}").contains("UPHEAP_PASSWORD_SIGN_IN"));
        Ok(())
    }

    #[test]
    fn passwords_off_needs_a_provider() {
        let error = config(&[BASE[0], BASE[1], ("UPHEAP_PASSWORD_SIGN_IN", "false")]).unwrap_err();
        assert!(format!("{error:#}").contains("UPHEAP_PASSWORD_SIGN_IN=false needs an OpenID Connect provider"));
    }

    #[test]
    fn reads_the_provider() -> anyhow::Result<()> {
        let vars: Vec<_> = BASE.iter().chain(&OIDC).copied().collect();
        let oidc = config(&vars)?.oidc.context("a provider")?;
        assert_eq!(oidc.label(), "Acme SSO");
        assert_eq!(oidc.issuer(), "https://idp.example.com/");
        assert_eq!(oidc.scopes(), ["email", "profile"]);

        let with_scopes: Vec<_> = vars
            .iter()
            .copied()
            .chain([("UPHEAP_OIDC_SCOPES", "openid  groups")])
            .collect();
        assert_eq!(config(&with_scopes)?.oidc.context("a provider")?.scopes(), ["groups"]);
        Ok(())
    }

    #[test]
    fn names_each_missing_provider_variable() {
        let vars: Vec<_> = BASE.iter().chain(&OIDC[..2]).copied().collect();
        let message = format!("{:#}", config(&vars).expect_err("an error"));
        assert!(message.contains("UPHEAP_OIDC_CLIENT_SECRET is required"), "{message}");
        assert!(message.contains("UPHEAP_OIDC_LABEL is required"), "{message}");
        assert!(!message.contains("UPHEAP_OIDC_CLIENT_ID is required"), "{message}");
    }

    fn with_groups(extra: &[(&'static str, &'static str)]) -> anyhow::Result<Config> {
        let vars: Vec<_> = BASE.iter().chain(&OIDC).chain(extra).copied().collect();
        config(&vars)
    }

    #[test]
    fn reads_the_group_teams_in_slug_order() -> anyhow::Result<()> {
        let json = r#"{ "sre": ["sre", "oncall"], "eng": ["engineering"] }"#;
        let teams = with_groups(&[("UPHEAP_OIDC_GROUP_TEAMS", json)])?.group_teams;
        assert_eq!(
            teams,
            [
                ("eng".to_owned(), vec!["engineering".to_owned()]),
                ("sre".to_owned(), vec!["sre".to_owned(), "oncall".to_owned()]),
            ]
        );
        Ok(())
    }

    #[test]
    fn has_no_group_teams_when_unset_or_blank() -> anyhow::Result<()> {
        assert!(with_groups(&[])?.group_teams.is_empty());
        assert!(
            with_groups(&[("UPHEAP_OIDC_GROUP_TEAMS", "  ")])?
                .group_teams
                .is_empty()
        );
        // Even without a provider, a blank value is no map.
        assert!(
            config(&[BASE[0], BASE[1], ("UPHEAP_OIDC_GROUP_TEAMS", "")])?
                .group_teams
                .is_empty()
        );
        Ok(())
    }

    #[test]
    fn provider_logout_is_off_unless_true_and_needs_the_provider() -> anyhow::Result<()> {
        let on = |extra: &[(&'static str, &'static str)]| -> anyhow::Result<bool> {
            Ok(with_groups(extra)?.oidc.context("a provider")?.provider_logout())
        };
        assert!(!on(&[])?);
        assert!(!on(&[("UPHEAP_OIDC_PROVIDER_LOGOUT", " ")])?);
        assert!(!on(&[("UPHEAP_OIDC_PROVIDER_LOGOUT", "false")])?);
        assert!(on(&[("UPHEAP_OIDC_PROVIDER_LOGOUT", "true")])?);

        let without_provider = config(&[BASE[0], BASE[1], ("UPHEAP_OIDC_PROVIDER_LOGOUT", "true")]);
        let message = format!("{:#}", without_provider.expect_err("no provider"));
        assert!(message.contains("UPHEAP_OIDC_PROVIDER_LOGOUT"), "{message}");
        let message = format!("{:#}", on(&[("UPHEAP_OIDC_PROVIDER_LOGOUT", "yes")]).expect_err("yes"));
        assert_eq!(
            message,
            "UPHEAP_OIDC_PROVIDER_LOGOUT must be true or false, not \"yes\""
        );
        Ok(())
    }

    #[test]
    fn the_subject_claim_defaults_to_sub_and_is_trimmed() -> anyhow::Result<()> {
        let claim = |extra: &[(&'static str, &'static str)]| -> anyhow::Result<String> {
            Ok(with_groups(extra)?
                .oidc
                .context("a provider")?
                .subject_claim()
                .to_owned())
        };
        assert_eq!(claim(&[])?, "sub");
        assert_eq!(claim(&[("UPHEAP_OIDC_SUBJECT_CLAIM", "   ")])?, "sub");
        assert_eq!(claim(&[("UPHEAP_OIDC_SUBJECT_CLAIM", " oid ")])?, "oid");
        Ok(())
    }

    #[test]
    fn the_groups_claim_defaults_to_groups_and_is_trimmed() -> anyhow::Result<()> {
        let claim = |extra: &[(&'static str, &'static str)]| -> anyhow::Result<String> {
            Ok(with_groups(extra)?
                .oidc
                .context("a provider")?
                .groups_claim()
                .to_owned())
        };
        assert_eq!(claim(&[])?, "groups");
        assert_eq!(claim(&[("UPHEAP_OIDC_GROUPS_CLAIM", "   ")])?, "groups");
        assert_eq!(claim(&[("UPHEAP_OIDC_GROUPS_CLAIM", " roles ")])?, "roles");
        Ok(())
    }

    #[test]
    fn rejects_a_group_map_that_cant_work() {
        let without_provider = config(&[BASE[0], BASE[1], ("UPHEAP_OIDC_GROUP_TEAMS", r#"{"eng":["a"]}"#)]);
        let mut errors = vec![without_provider.expect_err("no provider")];
        for json in ["not json", "[]", r#"{"eng":"a"}"#, r#"{"eng":[1]}"#, r#"{"eng":[]}"#] {
            errors.push(with_groups(&[("UPHEAP_OIDC_GROUP_TEAMS", json)]).expect_err(json));
        }
        for error in errors {
            let message = format!("{error:#}");
            assert!(message.contains("UPHEAP_OIDC_GROUP_TEAMS"), "{message}");
        }
    }

    #[test]
    fn rejects_a_malformed_issuer_or_an_empty_label() {
        for (name, value) in [("UPHEAP_OIDC_ISSUER", "not a url"), ("UPHEAP_OIDC_LABEL", "   ")] {
            let vars: Vec<_> = BASE.iter().chain(&OIDC).copied().filter(|(n, _)| *n != name).collect();
            let vars: Vec<_> = vars.into_iter().chain([(name, value)]).collect();
            assert!(config(&vars).is_err(), "{name}");
        }
    }

    const SCIM_TOKEN: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn has_no_scim_unless_a_token_is_set() -> anyhow::Result<()> {
        assert!(with_groups(&[])?.scim.is_none());
        assert!(with_groups(&[("UPHEAP_SCIM_TOKEN", "  ")])?.scim.is_none());
        assert!(config(&[BASE[0], BASE[1], ("UPHEAP_SCIM_TOKEN", "")])?.scim.is_none());
        Ok(())
    }

    #[test]
    fn reads_the_scim_token_without_showing_it() -> anyhow::Result<()> {
        let scim = with_groups(&[("UPHEAP_SCIM_TOKEN", SCIM_TOKEN)])?
            .scim
            .context("scim")?;
        assert!(!format!("{scim:?}").contains(SCIM_TOKEN));
        Ok(())
    }

    #[test]
    fn rejects_a_scim_token_that_is_too_short_or_has_no_provider() {
        let short = with_groups(&[("UPHEAP_SCIM_TOKEN", "0123456789abcdef0123456789abcde")]).expect_err("short");
        let without_provider = config(&[BASE[0], BASE[1], ("UPHEAP_SCIM_TOKEN", SCIM_TOKEN)]).expect_err("no provider");
        for error in [short, without_provider] {
            let message = format!("{error:#}");
            assert!(message.contains("UPHEAP_SCIM_TOKEN"), "{message}");
            assert!(!message.contains(SCIM_TOKEN), "{message}");
        }
    }

    #[test]
    fn reads_the_operator_contact_trimmed_and_none_when_blank() -> anyhow::Result<()> {
        assert!(with_groups(&[])?.operator_contact.is_none());
        assert!(
            with_groups(&[("UPHEAP_OPERATOR_CONTACT", "  ")])?
                .operator_contact
                .is_none()
        );
        let set = with_groups(&[("UPHEAP_OPERATOR_CONTACT", " Ask Sam in #it ")])?;
        assert_eq!(set.operator_contact.as_deref(), Some("Ask Sam in #it"));
        Ok(())
    }

    #[test]
    fn rejects_an_operator_contact_over_200_characters() {
        let long = "x".repeat(201);
        let error = config(&[BASE[0], BASE[1], ("UPHEAP_OPERATOR_CONTACT", long.as_str())]).expect_err("long");
        let message = format!("{error:#}");
        assert!(
            message.contains("UPHEAP_OPERATOR_CONTACT") && message.contains("200"),
            "{message}"
        );
    }
}
