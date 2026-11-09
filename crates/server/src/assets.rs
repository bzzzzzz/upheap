//! The web app, embedded at build time from `web/dist`. Unknown paths get `index.html`, so client-side routes
//! survive a reload.
//!
//! No page of the app may be shown in a frame on another site. Otherwise a page could frame the consent page that
//! connects the CLI, `/oauth/authorize`, under something that gets the user to click Allow (RFC 6749 §10.13). Every
//! response here says so, both the old way and the new: `X-Frame-Options` and CSP `frame-ancestors`.

use axum::http::{HeaderName, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "../../web/dist"]
#[allow_missing = true]
struct WebAssets;

/// What every response of the web app carries, so no other site can frame it.
const NO_FRAMING: [(HeaderName, &str); 2] = [
    (header::X_FRAME_OPTIONS, "DENY"),
    (header::CONTENT_SECURITY_POLICY, "frame-ancestors 'none'"),
];

pub async fn serve(uri: Uri) -> Response {
    (NO_FRAMING, serve_file(uri.path())).into_response()
}

fn serve_file(path: &str) -> Response {
    let path = path.trim_start_matches('/');
    if let Some(file) = WebAssets::get(path).filter(|_| !path.is_empty()) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        // Vite puts a content hash in every file name under assets/, so those can be cached forever.
        let cache = if path.starts_with("assets/") {
            "public, max-age=31536000, immutable"
        } else {
            "no-cache"
        };
        return (
            [(header::CONTENT_TYPE, mime.as_ref()), (header::CACHE_CONTROL, cache)],
            file.data,
        )
            .into_response();
    }
    match WebAssets::get("index.html") {
        Some(index) => (
            [
                (header::CONTENT_TYPE, "text/html; charset=utf-8"),
                (header::CACHE_CONTROL, "no-cache"),
            ],
            index.data,
        )
            .into_response(),
        None => (
            StatusCode::NOT_FOUND,
            "The web app wasn't built into this binary. Run `npm run build` in web/ and rebuild.",
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    #[gtest]
    #[tokio::test]
    async fn no_page_can_be_framed() -> googletest::Result<()> {
        // Built or not, the app answers these with index.html or the note that it's missing.
        for path in ["/", "/oauth/authorize?client_id=upheap-cli", "/login", "/favicon.svg"] {
            let response = serve(path.parse().or_fail()?).await;

            let headers = response.headers();
            expect_that!(
                headers.get(header::X_FRAME_OPTIONS).map(|v| v.as_bytes()),
                some(eq(b"DENY")),
                "{path}"
            );
            expect_that!(
                headers.get(header::CONTENT_SECURITY_POLICY).map(|v| v.as_bytes()),
                some(eq(b"frame-ancestors 'none'")),
                "{path}"
            );
        }
        Ok(())
    }
}
