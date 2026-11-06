//! The web app, embedded at build time from `web/dist`. Unknown paths get `index.html`, so client-side routes
//! survive a reload.

use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "../../web/dist"]
#[allow_missing = true]
struct WebAssets;

pub async fn serve(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
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
