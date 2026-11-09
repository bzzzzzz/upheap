//! The config file and the environment, read once at startup, and how flags, environment variables and the file
//! combine into the server and heap a command uses: a flag beats `UPHEAP_SERVER` and `UPHEAP_HEAP`, which beat the
//! file.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use reqwest::Url;
use serde::{Deserialize, Serialize};

/// `config.toml` in the CLI's config directory: what `upheap auth login --server` and `upheap heap use` write.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigFile {
    /// The server's base URL, where both the web app and `/api/v1` are.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
    /// The default heap, as `team/heap`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heap: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("the platform has no config directory for the CLI to keep its settings in")]
    NoConfigDir,
    #[error("can't read {}", path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{} isn't a valid config file", path.display())]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("can't write {}", path.display())]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// The config file in `dir`, or an empty one when there is none yet.
pub fn load_config(dir: &Path) -> Result<ConfigFile, ConfigError> {
    let path = dir.join(CONFIG_FILE);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(ConfigFile::default()),
        Err(source) => return Err(ConfigError::Read { path, source }),
    };
    toml::from_str(&text).map_err(|source| ConfigError::Parse { path, source })
}

/// Changes the config file in `dir` with `change`, and returns what `change` returns. Runs on the blocking pool, as
/// it waits for a lock and writes a file.
pub async fn update_config<T: Send + 'static>(
    dir: &Path,
    change: impl FnOnce(&mut ConfigFile) -> T + Send + 'static,
) -> Result<T, ConfigError> {
    let owned = dir.to_path_buf();
    tokio::task::spawn_blocking(move || update_config_now(&owned, change))
        .await
        .map_err(|err| ConfigError::Write {
            path: dir.join(CONFIG_FILE),
            source: std::io::Error::other(err),
        })?
}

/// [`update_config`], on this thread.
///
/// Holds an exclusive lock on `config.lock` in `dir` throughout, and reads the file again under it, so two
/// `upheap`s changing different settings at once both keep theirs. Writes a temporary file in the same directory
/// that then replaces the config, so a reader never sees half a file. Creates the directory when needed.
fn update_config_now<T>(dir: &Path, change: impl FnOnce(&mut ConfigFile) -> T) -> Result<T, ConfigError> {
    let lock_path = dir.join("config.lock");
    let lock = || -> std::io::Result<std::fs::File> {
        std::fs::create_dir_all(dir)?;
        let file = std::fs::File::options()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&lock_path)?;
        file.lock()?;
        Ok(file)
    };
    let _lock = lock().map_err(|source| ConfigError::Write {
        path: lock_path.clone(),
        source,
    })?;
    let mut config = load_config(dir)?;
    let changed = change(&mut config);
    let path = dir.join(CONFIG_FILE);
    let write = || -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        // Two plain strings always serialize.
        let text = toml::to_string(&config).map_err(std::io::Error::other)?;
        let mut file = tempfile::NamedTempFile::new_in(dir)?;
        std::io::Write::write_all(&mut file, text.as_bytes())?;
        file.persist(&path).map_err(|err| err.error)?;
        Ok(())
    };
    write().map_err(|source| ConfigError::Write {
        path: path.clone(),
        source,
    })?;
    Ok(changed)
}

const CONFIG_FILE: &str = "config.toml";

/// Why the command line, environment and config file don't say what a command needs. These are checked before any
/// request.
#[derive(Debug, thiserror::Error)]
pub enum SettingError {
    #[error("no server set: sign in with `upheap auth login --server <url>`, or set UPHEAP_SERVER")]
    NoServer,
    #[error("`{url}` isn't a server address: give one such as https://upheap.example.com")]
    BadServer { url: String },
    #[error(
        "`{url}` is plain http, which would send the sign-in's tokens unencrypted: use https, or http only to this machine (localhost, 127.0.0.1, ::1)"
    )]
    PlaintextServer { url: String },
    #[error(
        "no heap set: pick one with `upheap heap use <team/heap>`, or pass --heap or set UPHEAP_HEAP (`upheap heap list` lists them)"
    )]
    NoHeap,
}

/// The server to talk to: `flag`, then `env`, then `file`. The URL keeps any path the server is served under, without
/// a trailing slash, so it names the same sign-in however it was typed. Plain `http` is refused unless the server is
/// on this machine, as tokens would cross the network in the clear.
pub fn resolve_server(flag: Option<&str>, env: Option<&str>, file: Option<&str>) -> Result<Url, SettingError> {
    let raw = flag.or(env).or(file).ok_or(SettingError::NoServer)?;
    let bad = || SettingError::BadServer { url: raw.to_owned() };
    let mut url = Url::parse(raw.trim()).map_err(|_| bad())?;
    if !matches!(url.scheme(), "http" | "https") || url.host().is_none() || url.query().is_some() {
        return Err(bad());
    }
    if url.scheme() == "http" && !is_loopback(&url) {
        return Err(SettingError::PlaintextServer { url: raw.to_owned() });
    }
    url.set_fragment(None);
    let path = url.path().trim_end_matches('/').to_owned();
    url.set_path(&path);
    Ok(url)
}

/// Whether `url`'s host is this machine: `localhost`, `127.0.0.0/8` or `::1`.
pub fn is_loopback(url: &Url) -> bool {
    let host = url.host_str().unwrap_or_default();
    let ip = host.trim_start_matches('[').trim_end_matches(']');
    host.eq_ignore_ascii_case("localhost") || ip.parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

/// The heap a command works in, as typed: `flag`, then `env`, then `file`.
pub fn resolve_heap<'a>(
    flag: Option<&'a str>,
    env: Option<&'a str>,
    file: Option<&'a str>,
) -> Result<&'a str, SettingError> {
    flag.or(env).or(file).ok_or(SettingError::NoHeap)
}

/// The server's URL as the CLI keeps it: in the config file and as the key of its sign-in.
pub fn server_key(server: &Url) -> String {
    server.as_str().trim_end_matches('/').to_owned()
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    #[gtest]
    fn a_flag_beats_the_environment_which_beats_the_file() -> googletest::Result<()> {
        let server = |flag, env, file| resolve_server(flag, env, file).map(|url| server_key(&url));
        verify_that!(
            server(
                Some("https://flag.test"),
                Some("https://env.test"),
                Some("https://file.test")
            ),
            ok(eq("https://flag.test"))
        )?;
        verify_that!(
            server(None, Some("https://env.test"), Some("https://file.test")),
            ok(eq("https://env.test"))
        )?;
        verify_that!(
            server(None, None, Some("https://file.test")),
            ok(eq("https://file.test"))
        )?;
        verify_that!(resolve_heap(None, Some("a/b"), Some("c/d")), ok(eq(&"a/b")))
    }

    #[gtest]
    fn nothing_set_is_an_error_not_a_guess() -> googletest::Result<()> {
        verify_that!(
            resolve_server(None, None, None),
            err(matches_pattern!(SettingError::NoServer))
        )?;
        verify_that!(
            resolve_heap(None, None, None),
            err(matches_pattern!(SettingError::NoHeap))
        )
    }

    #[gtest]
    fn a_server_keeps_its_path_without_a_trailing_slash() -> googletest::Result<()> {
        let url = resolve_server(Some("https://example.test/upheap/"), None, None)?;
        verify_that!(server_key(&url), eq("https://example.test/upheap"))?;
        let root = resolve_server(Some("http://127.0.0.1:8080/"), None, None)?;
        verify_that!(server_key(&root), eq("http://127.0.0.1:8080"))
    }

    #[gtest]
    fn a_server_must_be_an_http_url() -> googletest::Result<()> {
        for raw in [
            "upheap.example.test",
            "ftp://example.test",
            "https://example.test/?x=1",
            "nonsense",
        ] {
            verify_that!(
                resolve_server(Some(raw), None, None),
                err(matches_pattern!(SettingError::BadServer { .. }))
            )?;
        }
        Ok(())
    }

    #[gtest]
    fn plain_http_is_only_for_this_machine() -> googletest::Result<()> {
        for raw in [
            "http://localhost:8080",
            "http://127.0.0.1:8080",
            "http://127.1.2.3",
            "http://[::1]:8080",
        ] {
            verify_that!(resolve_server(Some(raw), None, None), ok(anything()))?;
        }
        for raw in [
            "http://upheap.example.test",
            "http://192.168.1.10:8080",
            "http://localhost.example.test",
        ] {
            verify_that!(
                resolve_server(Some(raw), None, None),
                err(matches_pattern!(SettingError::PlaintextServer { .. }))
            )?;
        }
        Ok(())
    }

    #[gtest]
    fn the_config_file_round_trips_and_starts_empty() -> googletest::Result<()> {
        let dir = tempfile::tempdir()?;
        verify_that!(load_config(dir.path())?, eq(&ConfigFile::default()))?;
        let config = ConfigFile {
            server: Some("https://example.test".to_owned()),
            heap: Some("team/heap".to_owned()),
        };
        update_config_now(&dir.path().join("nested"), |file| *file = config.clone())?;
        verify_that!(load_config(&dir.path().join("nested"))?, eq(&config))
    }

    #[gtest]
    fn an_update_keeps_what_another_writer_saved_since() -> googletest::Result<()> {
        // Arrange: this `upheap` read an empty file at startup; another one has since saved a heap.
        let dir = tempfile::tempdir()?;
        update_config_now(dir.path(), |file| file.heap = Some("team/heap".to_owned()))?;

        // Act
        update_config_now(dir.path(), |file| file.server = Some("https://example.test".to_owned()))?;

        // Assert
        // The config and its lock, and no temporary file left behind.
        let leftovers = std::fs::read_dir(dir.path())?.count();
        verify_that!(leftovers, eq(2))?;
        verify_that!(
            load_config(dir.path())?,
            eq(&ConfigFile {
                server: Some("https://example.test".to_owned()),
                heap: Some("team/heap".to_owned()),
            })
        )
    }

    #[gtest]
    fn an_unknown_setting_is_named() -> googletest::Result<()> {
        let dir = tempfile::tempdir()?;
        std::fs::write(dir.path().join(CONFIG_FILE), "colour = \"always\"\n")?;

        let loaded = load_config(dir.path());

        let Err(ConfigError::Parse { source, .. }) = loaded else {
            return Err(googletest::internal::test_outcome::TestAssertionFailure::create(
                format!("{loaded:?}"),
            ));
        };
        verify_that!(source.to_string(), contains_substring("unknown field `colour`"))
    }

    #[gtest]
    fn a_broken_config_file_is_an_error() -> googletest::Result<()> {
        let dir = tempfile::tempdir()?;
        std::fs::write(dir.path().join(CONFIG_FILE), "server = [")?;
        verify_that!(
            load_config(dir.path()),
            err(matches_pattern!(ConfigError::Parse { .. }))
        )
    }
}
