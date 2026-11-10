//! The request and response types of the Upheap API, shared by the server's `api` crate, which serves them and
//! generates the OpenAPI document from them, and by clients written in Rust, such as the CLI.
//!
//! The types are the API's, not the domain's: they change for different reasons, even where fields match today.
//! Conversions between them and domain types live here, next to the types. Conversions from the service layer's
//! views, and anything that needs the request (its caller, its timezone), stay in `api`.
//!
//! Response types derive `Deserialize` as well as `Serialize`, so a client reads them back; request and query
//! types derive `Serialize` where a client sends them. This crate has no IO, and nothing in the workspace
//! but `domain` (outside it: serde, utoipa, jiff and uuid), so it builds for the browser and for the CLI alike.

pub mod audit;
pub mod auth;
pub mod common;
pub mod due_choices;
pub mod heaps;
pub mod invite_links;
pub mod items;
pub mod list_view;
pub mod me;
pub mod oauth;
pub mod parameters;
pub mod problem;
pub mod steps;
pub mod tags;
pub mod teams;
pub mod users;
pub mod views;
