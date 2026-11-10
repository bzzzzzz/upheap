//! What the commands that work in a heap share: the calls under the heap's path, finding the heap a command names,
//! and the description a flag gives.

use api_types::me::MeDto;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::client::{ApiClient, ClientError, Fetched, NO_QUERY};
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
    if raw != "-" {
        return Ok(raw.to_owned());
    }
    let text = app.read_stdin("--description").await?;
    let text = text.strip_suffix('\n').unwrap_or(&text);
    Ok(text.strip_suffix('\r').unwrap_or(text).to_owned())
}
