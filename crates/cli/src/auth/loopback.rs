//! The loopback redirect URI (RFC 8252 §7.3): a listener on 127.0.0.1 that waits for the browser to come back from
//! the consent page with a code, or with `error=access_denied`.
//!
//! It reads only the request line, so it needs no HTTP server: the browser sends a `GET` for the redirect URI, and
//! anything else it asks for, such as a favicon, gets `404`. Each connection is read in its own task with a short
//! timeout, because browsers open spare connections ahead of need (preconnects) that send nothing; waiting on one of
//! those must not hold up the one that brings the code.

use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

/// The path of the redirect URI.
const CALLBACK_PATH: &str = "/callback";
/// The most of a request the listener reads; a request line longer than this isn't from the consent page.
const MAX_REQUEST: usize = 16 * 1024;
/// How long one connection may take to send its request line.
const READ_TIMEOUT: Duration = Duration::from_secs(10);

/// A listener on a free port of 127.0.0.1.
pub struct Loopback {
    listener: TcpListener,
    redirect_uri: String,
}

/// The answer to this sign-in the browser brought back: its `state` was the request's.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Callback {
    pub code: Option<String>,
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

    /// Waits up to `timeout` for the browser to come to the redirect URI with `state`, and answers it with a page
    /// saying the terminal has it from here. A redirect with another `state` answers another sign-in, or none: it
    /// gets the not-signed-in page, and the wait goes on.
    pub async fn wait(self, state: &str, timeout: Duration) -> Result<Callback, LoopbackError> {
        tokio::time::timeout(timeout, self.accept_callback(Arc::from(state)))
            .await
            .map_err(|_| LoopbackError::TimedOut(timeout))
    }

    async fn accept_callback(self, state: Arc<str>) -> Callback {
        let (found, mut callbacks) = mpsc::channel(1);
        loop {
            tokio::select! {
                Some(callback) = callbacks.recv() => return callback,
                accepted = self.listener.accept() => {
                    let Ok((stream, _)) = accepted else { continue };
                    let (found, state) = (found.clone(), Arc::clone(&state));
                    tokio::spawn(async move {
                        match answer(stream, &state).await {
                            Ok(Some(callback)) => {
                                // The wait may have ended already; then nobody needs this one.
                                let _ = found.send(callback).await;
                            }
                            Ok(None) => {}
                            Err(err) => tracing::debug!(error = %err, "loopback request failed"),
                        }
                    });
                }
            }
        }
    }
}

/// Reads one request from `stream` and answers it: the callback when it's the redirect with `state`, or `None` for
/// anything else.
async fn answer(mut stream: TcpStream, state: &str) -> std::io::Result<Option<Callback>> {
    let target = tokio::time::timeout(READ_TIMEOUT, read_request_target(&mut stream))
        .await
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "no request line"))??;
    let query = target.as_deref().and_then(callback_query);
    let callback = query.as_ref().and_then(|query| query.answering(state));
    let (status, page) = match (&query, &callback) {
        (_, Some(callback)) if callback.code.is_some() && callback.error.is_none() => ("200 OK", SIGNED_IN_PAGE),
        (Some(_), _) => ("200 OK", NOT_SIGNED_IN_PAGE),
        (None, _) => ("404 Not Found", NOT_FOUND_PAGE),
    };
    if query.is_some() && callback.is_none() {
        tracing::warn!("the browser came back with another sign-in's state; still waiting");
    }
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

/// The redirect URI's query, before its `state` is checked.
#[derive(Default)]
struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

impl CallbackQuery {
    /// The callback, when the query answers the request with `state`.
    fn answering(&self, state: &str) -> Option<Callback> {
        (self.state.as_deref() == Some(state)).then(|| Callback {
            code: self.code.clone(),
            error: self.error.clone(),
        })
    }
}

/// The query of `target` when it is the redirect URI's path.
fn callback_query(target: &str) -> Option<CallbackQuery> {
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    if path != CALLBACK_PATH {
        return None;
    }
    let pairs: Vec<(String, String)> = serde_urlencoded::from_str(query).ok()?;
    let mut callback = CallbackQuery::default();
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

    fn port_of(redirect_uri: &str) -> String {
        redirect_uri
            .trim_start_matches("http://127.0.0.1:")
            .split('/')
            .next()
            .unwrap_or_default()
            .to_owned()
    }

    /// Sends `GET <target>` to the redirect URI's port as a browser would, and returns the response.
    async fn browse(redirect_uri: &str, target: &str) -> std::io::Result<String> {
        let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port_of(redirect_uri))).await?;
        stream
            .write_all(format!("GET {target} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n").as_bytes())
            .await?;
        let mut response = String::new();
        stream.read_to_string(&mut response).await?;
        Ok(response)
    }

    #[gtest]
    #[tokio::test]
    async fn the_browser_brings_back_the_code() -> googletest::Result<()> {
        // Arrange
        let loopback = Loopback::bind().await?;
        let uri = loopback.redirect_uri().to_owned();
        let waiting = tokio::spawn(async move { loopback.wait("xyz", Duration::from_secs(10)).await });

        // Act
        let favicon = browse(&uri, "/favicon.ico").await?;
        let page = browse(&uri, "/callback?code=abc%2Bdef&state=xyz").await?;
        let callback = waiting.await??;

        // Assert
        expect_that!(uri, starts_with("http://127.0.0.1:"));
        expect_that!(favicon, starts_with("HTTP/1.1 404"));
        expect_that!(page, contains_substring("The CLI has the sign-in"));
        expect_that!(
            callback,
            eq(&Callback {
                code: Some("abc+def".to_owned()),
                error: None,
            })
        );
        Ok(())
    }

    #[gtest]
    #[tokio::test]
    async fn a_cancel_comes_back_as_access_denied() -> googletest::Result<()> {
        let loopback = Loopback::bind().await?;
        let uri = loopback.redirect_uri().to_owned();
        let waiting = tokio::spawn(async move { loopback.wait("xyz", Duration::from_secs(10)).await });

        let page = browse(&uri, "/callback?error=access_denied&state=xyz").await?;
        let callback = waiting.await??;

        expect_that!(page, contains_substring("wasn't signed in"));
        expect_that!(callback.error.as_deref(), some(eq("access_denied")));
        expect_that!(callback.code, none());
        Ok(())
    }

    #[gtest]
    #[tokio::test]
    async fn an_idle_connection_doesnt_hold_up_the_callback() -> googletest::Result<()> {
        // Arrange: a browser's preconnect, open and silent for longer than the test waits.
        let loopback = Loopback::bind().await?;
        let uri = loopback.redirect_uri().to_owned();
        let waiting = tokio::spawn(async move { loopback.wait("xyz", Duration::from_secs(5)).await });
        let _idle = TcpStream::connect(format!("127.0.0.1:{}", port_of(&uri))).await?;

        // Act
        let page = browse(&uri, "/callback?code=c0de&state=xyz").await?;
        let callback = waiting.await??;

        // Assert
        expect_that!(page, contains_substring("The CLI has the sign-in"));
        expect_that!(callback.code.as_deref(), some(eq("c0de")));
        Ok(())
    }

    #[gtest]
    #[tokio::test]
    async fn another_sign_ins_state_is_turned_away_and_the_wait_goes_on() -> googletest::Result<()> {
        // Arrange
        let loopback = Loopback::bind().await?;
        let uri = loopback.redirect_uri().to_owned();
        let waiting = tokio::spawn(async move { loopback.wait("xyz", Duration::from_secs(10)).await });

        // Act
        let wrong = browse(&uri, "/callback?code=stolen&state=other").await?;
        let missing = browse(&uri, "/callback?code=stolen").await?;
        let right = browse(&uri, "/callback?code=c0de&state=xyz").await?;
        let callback = waiting.await??;

        // Assert
        expect_that!(wrong, contains_substring("wasn't signed in"));
        expect_that!(missing, contains_substring("wasn't signed in"));
        expect_that!(right, contains_substring("The CLI has the sign-in"));
        expect_that!(callback.code.as_deref(), some(eq("c0de")));
        Ok(())
    }

    #[gtest]
    #[tokio::test]
    async fn waiting_gives_up_after_the_timeout() -> googletest::Result<()> {
        let loopback = Loopback::bind().await?;

        let waited = loopback.wait("xyz", Duration::from_millis(50)).await;

        expect_that!(waited, err(matches_pattern!(LoopbackError::TimedOut(_))));
        Ok(())
    }
}
