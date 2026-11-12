//! What the commands that work in a heap share: the calls under the heap's path, finding the heap a command names,
//! and the description and context a flag gives.

use api_types::me::MeDto;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::client::{ApiClient, ClientError, Fetched, NO_QUERY};
use crate::commands::UsageError;
use crate::config::SettingError;
use crate::lookup::{HeapHandle, HeapName, resolve_heap};
use crate::{App, CliError};

/// The calls under one heap's path, as one caller.
pub(crate) struct HeapApi<'a> {
    pub client: &'a ApiClient,
    pub token: &'a str,
    pub heap: &'a HeapHandle,
}

impl HeapApi<'_> {
    /// `GET` of `resource` under the heap, such as `["items"]`.
    pub async fn get<T: DeserializeOwned>(
        &self,
        resource: &[&str],
        query: &(impl Serialize + ?Sized),
    ) -> Result<Fetched<T>, CliError> {
        Ok(self.client.get(&self.path(resource), query, self.token).await?)
    }

    /// `POST` of `body` to `resource` under the heap.
    pub async fn post<T: DeserializeOwned>(
        &self,
        resource: &[&str],
        body: &(impl Serialize + ?Sized),
    ) -> Result<Fetched<T>, CliError> {
        Ok(self.client.post(&self.path(resource), body, self.token).await?)
    }

    /// `PATCH` of `body` to `resource` under the heap, which is at `version`.
    pub async fn patch<T: DeserializeOwned>(
        &self,
        resource: &[&str],
        version: u32,
        body: &(impl Serialize + ?Sized),
    ) -> Result<Fetched<T>, CliError> {
        Ok(self
            .client
            .patch(&self.path(resource), body, version, self.token)
            .await?)
    }

    /// `POST` of `body` to `resource` under the heap, which is at `version`.
    pub async fn post_at_version<T: DeserializeOwned>(
        &self,
        resource: &[&str],
        version: u32,
        body: &(impl Serialize + ?Sized),
    ) -> Result<Fetched<T>, CliError> {
        Ok(self
            .client
            .post_at_version(&self.path(resource), body, version, self.token)
            .await?)
    }

    /// `PUT` of `body` to `resource` under the heap, which is at `version`.
    pub async fn put<T: DeserializeOwned>(
        &self,
        resource: &[&str],
        version: u32,
        body: &(impl Serialize + ?Sized),
    ) -> Result<Fetched<T>, CliError> {
        Ok(self
            .client
            .put_at_version(&self.path(resource), body, version, self.token)
            .await?)
    }

    /// The path of `resource` under the heap.
    pub fn path<'p>(&'p self, resource: &[&'p str]) -> Vec<&'p str> {
        let mut path = vec!["teams", self.heap.team.as_str(), "heaps", self.heap.heap.as_str()];
        path.extend_from_slice(resource);
        path
    }
}

/// Whether the server answered `404`: the thing isn't there, or the heap isn't the caller's, which it doesn't tell
/// apart.
pub(crate) fn is_not_found(err: &ClientError) -> bool {
    matches!(err, ClientError::Problem { problem, .. } if problem.status == 404)
}

/// The handle of the heap `name` names: complete as typed, or looked up among the caller's heaps in `GET /me`.
pub(crate) async fn resolve_heap_handle(
    name: HeapName<'_>,
    client: &ApiClient,
    token: &str,
) -> Result<HeapHandle, CliError> {
    if let Some(handle) = name.to_handle() {
        return Ok(handle);
    }
    let me = client.get::<MeDto>(&["me"], NO_QUERY, token).await?;
    Ok(resolve_heap(name, &me.value)?)
}

/// The heap a command works in, from `--heap`, the environment or the config file, as typed.
pub(crate) fn named_heap<'a>(flag: Option<&'a str>, app: &'a App) -> Result<HeapName<'a>, CliError> {
    let typed = app.heap_name(flag).ok_or(SettingError::NoHeap)?;
    Ok(HeapName::parse(typed)?)
}

/// The description a flag gives: its text, or all of stdin for `-`, less the newline that ends it.
pub(crate) async fn read_description(raw: &str, app: &App) -> Result<String, CliError> {
    read_text_flag(raw, "--description", app).await
}

/// The text a flag called `flag` gives: its value, or all of stdin for `-`, less the newline that ends it.
pub(crate) async fn read_text_flag(raw: &str, flag: &'static str, app: &App) -> Result<String, CliError> {
    if raw != "-" {
        return Ok(raw.to_owned());
    }
    let text = app.read_stdin(flag).await?;
    let text = text.strip_suffix('\n').unwrap_or(&text);
    Ok(text.strip_suffix('\r').unwrap_or(text).to_owned())
}

/// The context a flag gives: the text of the file at `path`, or all of stdin for `-`.
pub(crate) async fn read_context(path: &str, app: &App) -> Result<String, CliError> {
    if path == "-" {
        return Ok(app.read_stdin("--context-file").await?);
    }
    std::fs::read_to_string(path).map_err(|source| {
        UsageError::UnreadableContextFile {
            path: path.to_owned(),
            source,
        }
        .into()
    })
}

/// A context's lines for printing under a heading, each indented two spaces. One line at a time, so the newlines
/// stay and anything else that moves the cursor doesn't. `lines` also drops the `\r` of CRLF text, and a tab becomes
/// spaces, as the output would otherwise show it as U+FFFD.
pub(crate) fn indented_context(text: &str) -> impl Iterator<Item = String> {
    text.lines().map(|line| format!("  {}", line.replace('\t', "    ")))
}
