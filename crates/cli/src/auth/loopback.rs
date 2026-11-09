//! The loopback redirect URI (RFC 8252 §7.3): a listener on 127.0.0.1 that waits for the browser to come back from
//! the consent page with a code, or with `error=access_denied`.
//!
//! It reads only the request line, so it needs no HTTP server: the browser sends a `GET` for the redirect URI, and
//! anything else it asks for, such as a favicon, gets `404`.

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// The path of the redirect URI.
const CALLBACK_PATH: &str = "/callback";
/// The most of a request the listener reads; a request line longer than this isn't from the consent page.
const MAX_REQUEST: usize = 16 * 1024;

/// A listener on a free port of 127.0.0.1.
pub struct Loopback {
    listener: TcpListener,
    redirect_uri: String,
}

/// The query the browser brought back to the redirect URI.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Callback {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum LoopbackError {
    #[error("can't listen on a loopback port for the browser to come back to")]
    Bind(#[source] std::io::Error),
    #[error("gave up waiting for the browser after {} minutes", .0.as_secs() / 60)]
    TimedOut(Duration),
}

impl Loopback {
    /// Listens on a port the system picks.
    pub async fn bind() -> Result<Self, LoopbackError> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.map_err(LoopbackError::Bind)?;
        let port = listener.local_addr().map_err(LoopbackError::Bind)?.port();
        Ok(Self {
            listener,
            redirect_uri: format!("http://127.0.0.1:{port}{CALLBACK_PATH}"),
        })
    }

    /// `http://127.0.0.1:<port>/callback`.
    pub fn redirect_uri(&self) -> &str {
        &self.redirect_uri
    }

    /// Waits up to `timeout` for the browser to come to the redirect URI, and answers it with a page saying the
    /// terminal has it from here.
    pub async fn wait(self, timeout: Duration) -> Result<Callback, LoopbackError> {
        tokio::time::timeout(timeout, self.accept_callback())
            .await
            .map_err(|_| LoopbackError::TimedOut(timeout))
    }

    async fn accept_callback(self) -> Callback {
        loop {
            let Ok((stream, _)) = self.listener.accept().await else {
                continue;
            };
            match answer(stream).await {
                Ok(Some(callback)) => return callback,
                Ok(None) => {}
                Err(err) => tracing::debug!(error = %err, "loopback request failed"),
            }
        }
    }
}

/// Reads one request from `stream` and answers it: the callback, or `None` for anything else.
async fn answer(mut stream: TcpStream) -> std::io::Result<Option<Callback>> {
    let target = read_request_target(&mut stream).await?;
    let callback = target.as_deref().and_then(parse_callback);
    let (status, page) = match &callback {
        Some(callback) if callback.code.is_some() && callback.error.is_none() => ("200 OK", SIGNED_IN_PAGE),
        Some(_) => ("200 OK", NOT_SIGNED_IN_PAGE),
        None => ("404 Not Found", NOT_FOUND_PAGE),
    };
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n{page}",
        page.len()
    );
    stream.write_all(response.as_bytes()).await?;
    stream.shutdown().await?;
    Ok(callback)
}

/// The target of a `GET` request line, such as `/callback?code=…`, or `None` for another method or a request that
/// doesn't parse.
async fn read_request_target(stream: &mut TcpStream) -> std::io::Result<Option<String>> {
    let mut request = Vec::new();
    let mut chunk = [0_u8; 2048];
    while !request.windows(4).any(|w| w == b"\r\n\r\n") && request.len() < MAX_REQUEST {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            break;
        }
        request.extend_from_slice(&chunk[..read]);
    }
    let text = String::from_utf8_lossy(&request);
    let mut words = text.lines().next().unwrap_or_default().split(' ');
    Ok(match (words.next(), words.next()) {
        (Some("GET"), Some(target)) => Some(target.to_owned()),
        _ => None,
    })
}

/// The callback's query when `target` is the redirect URI's path.
fn parse_callback(target: &str) -> Option<Callback> {
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    if path != CALLBACK_PATH {
        return None;
    }
    let pairs: Vec<(String, String)> = serde_urlencoded::from_str(query).ok()?;
    let mut callback = Callback::default();
    for (name, value) in pairs {
        match name.as_str() {
            "code" => callback.code = Some(value),
            "state" => callback.state = Some(value),
            "error" => callback.error = Some(value),
            _ => {}
        }
    }
    Some(callback)
}

const SIGNED_IN_PAGE: &str = "<!doctype html><meta charset=utf-8><title>Upheap CLI</title>\
<p style=\"font-family:system-ui,sans-serif\">The CLI has the sign-in. You can close this tab and go back to the terminal.</p>";
const NOT_SIGNED_IN_PAGE: &str = "<!doctype html><meta charset=utf-8><title>Upheap CLI</title>\
<p style=\"font-family:system-ui,sans-serif\">The CLI wasn't signed in. You can close this tab; the terminal says why.</p>";
const NOT_FOUND_PAGE: &str = "<!doctype html><meta charset=utf-8><title>Not found</title>";

#[cfg(test)]
mod tests {
    use googletest::prelude::*;
    use tokio::io::AsyncReadExt;

    use super::*;

    /// Sends `GET <target>` to `uri`'s port as a browser would, and returns the response.
    async fn browse(redirect_uri: &str, target: &str) -> std::io::Result<String> {
        let port = redirect_uri
            .trim_start_matches("http://127.0.0.1:")
            .split('/')
            .next()
            .unwrap_or_default()
            .to_owned();
        let mut stream = TcpStream::connect(format!("127.0.0.1:{port}")).await?;
        stream
            .write_all(format!("GET {target} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n").as_bytes())
            .await?;
        let mut response = String::new();
        stream.read_to_string(&mut response).await?;
        Ok(response)
    }

    #[tokio::test]
    async fn the_browser_brings_back_the_code_and_state() -> googletest::Result<()> {
        // Arrange
        let loopback = Loopback::bind().await?;
        let uri = loopback.redirect_uri().to_owned();
        let waiting = tokio::spawn(loopback.wait(Duration::from_secs(10)));

        // Act
        let favicon = browse(&uri, "/favicon.ico").await?;
        let page = browse(&uri, "/callback?code=abc%2Bdef&state=xyz").await?;
        let callback = waiting.await??;

        // Assert
        verify_that!(uri, starts_with("http://127.0.0.1:"))?;
        verify_that!(favicon, starts_with("HTTP/1.1 404"))?;
        verify_that!(page, contains_substring("close this tab"))?;
        verify_that!(
            callback,
            eq(&Callback {
                code: Some("abc+def".to_owned()),
                state: Some("xyz".to_owned()),
                error: None,
            })
        )
    }

    #[tokio::test]
    async fn a_cancel_comes_back_as_access_denied() -> googletest::Result<()> {
        let loopback = Loopback::bind().await?;
        let uri = loopback.redirect_uri().to_owned();
        let waiting = tokio::spawn(loopback.wait(Duration::from_secs(10)));

        let page = browse(&uri, "/callback?error=access_denied&state=xyz").await?;
        let callback = waiting.await??;

        verify_that!(page, contains_substring("wasn't signed in"))?;
        verify_that!(callback.error.as_deref(), some(eq("access_denied")))?;
        verify_that!(callback.code, none())
    }

    #[tokio::test]
    async fn waiting_gives_up_after_the_timeout() -> googletest::Result<()> {
        let loopback = Loopback::bind().await?;

        let waited = loopback.wait(Duration::from_millis(50)).await;

        verify_that!(waited, err(matches_pattern!(LoopbackError::TimedOut(_))))
    }
}
