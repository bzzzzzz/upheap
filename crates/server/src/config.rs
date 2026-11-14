//! Configuration, read once from environment variables at startup and validated before anything else runs.

use std::net::SocketAddr;

use anyhow::{Context, bail};
use service::oidc::OidcSettings;

/// Every option the server reads. Documented in the README; validated by [`Config::from_env`].
#[derive(Debug)]
pub struct Config {
    /// `DATABASE_URL`, required: `sqlite://path/to/heap.db?mode=rwc`, `mysql://user:pass@host/db` or
    /// `postgres://user:pass@host/db`.
    pub database_url: String,
    /// `DATABASE_MAX_CONNECTIONS`, default `5`: the most database connections this server keeps open. Small, because
    /// on Cloud Run several instances may share one database's connection limit.
    pub database_max_connections: u32,
    /// `BASE_URL`, required: the public URL users open, for example `https://heap.example.com`.
    /// Session cookies are `Secure` unless it starts with `http://`.
    pub base_url: String,
    /// `BIND_ADDR`, default `0.0.0.0:8080`.
    pub bind_addr: SocketAddr,
    /// `ALLOW_REGISTRATION`, default `true`.
    pub allow_registration: bool,
    /// `SESSION_TTL_DAYS`, default `30`.
    pub session_ttl_days: u16,
    /// `LOG_FORMAT`: `json` (default) or `pretty`.
    pub log_format: LogFormat,
    /// `UPHEAP_OIDC_ISSUER`, `UPHEAP_OIDC_CLIENT_ID`, `UPHEAP_OIDC_CLIENT_SECRET` and `UPHEAP_OIDC_LABEL`, all or
    /// none, and `UPHEAP_OIDC_SCOPES`, default `openid email profile`: the provider people may sign in through. No
    /// request is made to it at startup.
    pub oidc: Option<OidcSettings>,
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
        Ok(Self {
            database_url,
            database_max_connections,
            base_url,
            bind_addr,
            allow_registration,
            session_ttl_days,
            log_format,
            oidc,
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
        .context("UPHEAP_OIDC_ISSUER, UPHEAP_OIDC_LABEL or BASE_URL is not usable for the OpenID Connect provider")?;
    Ok(Some(settings))
}

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

    #[test]
    fn rejects_a_malformed_issuer_or_an_empty_label() {
        for (name, value) in [("UPHEAP_OIDC_ISSUER", "not a url"), ("UPHEAP_OIDC_LABEL", "   ")] {
            let vars: Vec<_> = BASE.iter().chain(&OIDC).copied().filter(|(n, _)| *n != name).collect();
            let vars: Vec<_> = vars.into_iter().chain([(name, value)]).collect();
            assert!(config(&vars).is_err(), "{name}");
        }
    }
}
