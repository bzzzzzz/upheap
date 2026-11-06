//! Every endpoint, called by every kind of caller and against an archived heap.
//!
//! The table below only says which permission each endpoint needs. Expected statuses are derived from
//! `Permission::is_granted_to`, the function the service checks, so the test and the roles table can't drift.
//! An endpoint in the OpenAPI document that is missing from the table fails `every_endpoint_is_in_the_table`.

mod common;

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicU32, Ordering};

use anyhow::{Context, Result};
use axum::http::{Method, StatusCode};
use common::{TestApp, TestResponse};
use domain::permission::Permission;
use domain::team::Role;
use googletest::prelude::*;
use serde_json::{Value, json};

const PASSWORD: &str = "a long test password";

/// Who an endpoint is open to.
#[derive(Clone, Copy)]
enum Access {
    /// Anyone, signed in or not.
    Public,
    /// Any signed-in user; nothing is scoped to a team.
    SignedIn,
    /// Members of the heap's team holding `permission`. Writes are refused while the heap is archived.
    Heap { permission: Permission, writes: bool },
}

/// What a request needs to exist before it is sent.
#[derive(Clone, Copy)]
enum Target {
    Nothing,
    Heap,
    OpenItem,
    ClosedItem,
    Tag,
    /// A session of its own, so signing out doesn't end the caller's shared session.
    FreshSession,
}

/// The body a request carries.
#[derive(Clone, Copy)]
enum RequestBody {
    Empty,
    Registration,
    SignIn,
    NewItem,
    Rename,
    /// A tag name no other request used, since names are unique per heap.
    NewTag,
    RenameTag,
}

struct Endpoint {
    method: Method,
    /// Path under `/api/v1`, exactly as the OpenAPI document spells it.
    path: &'static str,
    access: Access,
    target: Target,
    body: RequestBody,
    /// Status of a successful call.
    success: StatusCode,
}

fn endpoints() -> Vec<Endpoint> {
    use Access::{Heap, Public, SignedIn};
    use Permission::{EditItems, ViewHeap};
    let view = Heap {
        permission: ViewHeap,
        writes: false,
    };
    let edit = Heap {
        permission: EditItems,
        writes: true,
    };
    let row = |method, path, access, target, body, success| Endpoint {
        method,
        path,
        access,
        target,
        body,
        success,
    };
    vec![
        row(
            Method::POST,
            "/auth/register",
            Public,
            Target::Nothing,
            RequestBody::Registration,
            StatusCode::CREATED,
        ),
        row(
            Method::POST,
            "/auth/login",
            Public,
            Target::Nothing,
            RequestBody::SignIn,
            StatusCode::OK,
        ),
        row(
            Method::POST,
            "/auth/logout",
            Public,
            Target::FreshSession,
            RequestBody::Empty,
            StatusCode::NO_CONTENT,
        ),
        row(
            Method::GET,
            "/me",
            SignedIn,
            Target::Nothing,
            RequestBody::Empty,
            StatusCode::OK,
        ),
        row(
            Method::GET,
            "/heaps/{heap_id}/items",
            view,
            Target::Heap,
            RequestBody::Empty,
            StatusCode::OK,
        ),
        row(
            Method::GET,
            "/heaps/{heap_id}/items/closed",
            view,
            Target::Heap,
            RequestBody::Empty,
            StatusCode::OK,
        ),
        row(
            Method::POST,
            "/heaps/{heap_id}/items",
            edit,
            Target::Heap,
            RequestBody::NewItem,
            StatusCode::CREATED,
        ),
        row(
            Method::GET,
            "/items/{item_id}",
            view,
            Target::OpenItem,
            RequestBody::Empty,
            StatusCode::OK,
        ),
        row(
            Method::PATCH,
            "/items/{item_id}",
            edit,
            Target::OpenItem,
            RequestBody::Rename,
            StatusCode::OK,
        ),
        row(
            Method::POST,
            "/items/{item_id}/reopen",
            edit,
            Target::ClosedItem,
            RequestBody::Empty,
            StatusCode::OK,
        ),
        row(
            Method::GET,
            "/heaps/{heap_id}/tags",
            view,
            Target::Heap,
            RequestBody::Empty,
            StatusCode::OK,
        ),
        row(
            Method::POST,
            "/heaps/{heap_id}/tags",
            edit,
            Target::Heap,
            RequestBody::NewTag,
            StatusCode::CREATED,
        ),
        row(
            Method::PATCH,
            "/tags/{tag_id}",
            edit,
            Target::Tag,
            RequestBody::RenameTag,
            StatusCode::OK,
        ),
    ]
}

#[derive(Clone, Copy, Debug)]
enum Caller {
    Anonymous,
    Outsider,
    Member,
    Admin,
}

/// What a call should return: a status, plus the problem `code` for errors.
#[derive(Debug, PartialEq)]
struct Outcome {
    status: StatusCode,
    code: Option<String>,
}

impl Outcome {
    fn success(status: StatusCode) -> Self {
        Self { status, code: None }
    }

    fn problem(status: StatusCode, code: &str) -> Self {
        Self {
            status,
            code: Some(code.to_owned()),
        }
    }

    fn of(response: &TestResponse) -> Self {
        let code = if response.status.is_success() {
            None
        } else {
            response.body["code"].as_str().map(str::to_owned)
        };
        Self {
            status: response.status,
            code,
        }
    }
}

fn expected_outcome(endpoint: &Endpoint, caller: Caller, archived: bool) -> Outcome {
    let (permission, writes) = match endpoint.access {
        Access::Public => return Outcome::success(endpoint.success),
        Access::SignedIn | Access::Heap { .. } if matches!(caller, Caller::Anonymous) => {
            return Outcome::problem(StatusCode::UNAUTHORIZED, "unauthenticated");
        }
        Access::SignedIn => return Outcome::success(endpoint.success),
        Access::Heap { permission, writes } => (permission, writes),
    };
    let role = match caller {
        Caller::Anonymous => return Outcome::problem(StatusCode::UNAUTHORIZED, "unauthenticated"),
        // Non-members can't tell a heap they may not see from one that doesn't exist.
        Caller::Outsider => return Outcome::problem(StatusCode::NOT_FOUND, "not_found"),
        Caller::Member => Role::Member,
        Caller::Admin => Role::Admin,
    };
    if !permission.is_granted_to(role) {
        return Outcome::problem(StatusCode::FORBIDDEN, "forbidden");
    }
    if archived && writes {
        return Outcome::problem(StatusCode::CONFLICT, "heap_archived");
    }
    Outcome::success(endpoint.success)
}

struct Account {
    email: String,
    session: String,
}

/// A team with an admin and a member sharing one heap, plus a user from another team.
struct World {
    app: TestApp,
    admin: Account,
    member: Account,
    outsider: Account,
    heap: String,
    /// Counter for names that must be unique: emails and tag names.
    unique_names: AtomicU32,
}

impl World {
    async fn build() -> Result<Self> {
        let app = TestApp::start().await?;
        let (admin_session, heap) = app.register("admin@example.test").await?;
        let (member_session, _) = app.register("member@example.test").await?;
        let (outsider_session, _) = app.register("outsider@example.test").await?;

        let admin_me = app.send(Method::GET, "/api/v1/me", Some(&admin_session), None).await?;
        let team = admin_me.body["teams"][0]["id"].as_str().context("admin has no team")?;
        let member_me = app.send(Method::GET, "/api/v1/me", Some(&member_session), None).await?;
        let member_id = member_me.body["user"]["id"].as_str().context("member has no id")?;
        app.add_member(team, member_id, Role::Member).await?;

        Ok(Self {
            app,
            admin: Account {
                email: "admin@example.test".to_owned(),
                session: admin_session,
            },
            member: Account {
                email: "member@example.test".to_owned(),
                session: member_session,
            },
            outsider: Account {
                email: "outsider@example.test".to_owned(),
                session: outsider_session,
            },
            heap,
            unique_names: AtomicU32::new(0),
        })
    }

    fn account(&self, caller: Caller) -> Option<&Account> {
        match caller {
            Caller::Anonymous => None,
            Caller::Outsider => Some(&self.outsider),
            Caller::Member => Some(&self.member),
            Caller::Admin => Some(&self.admin),
        }
    }

    /// Creates an item as the admin, closing it when `closed`, and returns its ID and version.
    async fn create_item(&self, closed: bool) -> Result<(String, String)> {
        let created = self
            .app
            .send(
                Method::POST,
                &format!("/api/v1/heaps/{}/items", self.heap),
                Some(&self.admin.session),
                Some(json!({ "title": "Target" })),
            )
            .await?;
        let id = created.body["id"].as_str().context("item has no id")?.to_owned();
        if !closed {
            return Ok((id, created.body["version"].to_string()));
        }
        let done = self
            .app
            .send_with(
                Method::PATCH,
                &format!("/api/v1/items/{id}"),
                Some(&self.admin.session),
                Some(json!({ "state": "done" })),
                &[
                    ("x-csrf-protection", "1"),
                    ("if-match", &created.body["version"].to_string()),
                ],
            )
            .await?;
        Ok((id, done.body["version"].to_string()))
    }

    /// Creates a tag as the admin and returns its ID and version.
    async fn create_tag(&self) -> Result<(String, String)> {
        let created = self
            .app
            .send(
                Method::POST,
                &format!("/api/v1/heaps/{}/tags", self.heap),
                Some(&self.admin.session),
                Some(json!({ "name": self.unique_name("target") })),
            )
            .await?;
        let id = created.body["id"].as_str().context("tag has no id")?.to_owned();
        Ok((id, created.body["version"].to_string()))
    }

    fn unique_name(&self, prefix: &str) -> String {
        format!("{prefix}{}", self.unique_names.fetch_add(1, Ordering::Relaxed))
    }

    async fn sign_in(&self, account: &Account) -> Result<String> {
        let response = self
            .app
            .send(
                Method::POST,
                "/api/v1/auth/login",
                None,
                Some(json!({ "email": account.email, "password": PASSWORD })),
            )
            .await?;
        response.session_cookie.context("sign-in set no cookie")
    }

    fn body(&self, body: RequestBody) -> Option<Value> {
        match body {
            RequestBody::Empty => None,
            RequestBody::Registration => {
                let email = format!("{}@example.test", self.unique_name("new"));
                Some(json!({ "email": email, "display_name": "New", "password": PASSWORD }))
            }
            RequestBody::SignIn => Some(json!({ "email": self.admin.email, "password": PASSWORD })),
            RequestBody::NewItem => Some(json!({ "title": "New" })),
            RequestBody::Rename => Some(json!({ "title": "Renamed" })),
            RequestBody::NewTag => Some(json!({ "name": self.unique_name("new") })),
            RequestBody::RenameTag => Some(json!({ "name": self.unique_name("renamed") })),
        }
    }

    /// Prepares what `endpoint` needs, then calls it as `caller`, with the heap archived for the call when asked.
    async fn call(&self, endpoint: &Endpoint, caller: Caller, archived: bool) -> Result<TestResponse> {
        let account = self.account(caller);
        let mut session = account.map(|a| a.session.clone());
        let mut path = endpoint.path.replace("{heap_id}", &self.heap);
        let mut if_match = None;
        match endpoint.target {
            Target::Nothing | Target::Heap => {}
            Target::OpenItem | Target::ClosedItem => {
                let (id, version) = self.create_item(matches!(endpoint.target, Target::ClosedItem)).await?;
                path = path.replace("{item_id}", &id);
                if_match = Some(version);
            }
            Target::Tag => {
                let (id, version) = self.create_tag().await?;
                path = path.replace("{tag_id}", &id);
                if_match = Some(version);
            }
            Target::FreshSession => {
                session = match account {
                    Some(account) => Some(self.sign_in(account).await?),
                    None => None,
                };
            }
        }
        let mut headers = vec![("x-csrf-protection", "1")];
        if let Some(version) = &if_match {
            headers.push(("if-match", version));
        }

        if archived {
            self.app.set_heap_archived(&self.heap, true).await?;
        }
        let response = self
            .app
            .send_with(
                endpoint.method.clone(),
                &format!("/api/v1{path}"),
                session.as_deref(),
                self.body(endpoint.body),
                &headers,
            )
            .await;
        if archived {
            self.app.set_heap_archived(&self.heap, false).await?;
        }
        response
    }
}

#[gtest]
#[tokio::test]
async fn every_endpoint_answers_each_caller_as_the_roles_table_says() -> Result<()> {
    // Arrange
    let world = World::build().await?;
    let callers = [Caller::Anonymous, Caller::Outsider, Caller::Member, Caller::Admin];
    let mut mismatches = Vec::new();

    // Act
    for endpoint in endpoints() {
        for caller in callers {
            let response = world.call(&endpoint, caller, false).await?;
            let (got, want) = (Outcome::of(&response), expected_outcome(&endpoint, caller, false));
            if got != want {
                mismatches.push(format!(
                    "{} {} as {caller:?}: got {got:?}, want {want:?}",
                    endpoint.method, endpoint.path
                ));
            }
        }
    }

    // Assert
    expect_that!(mismatches, is_empty());
    Ok(())
}

#[gtest]
#[tokio::test]
async fn archived_heaps_refuse_writes_and_allow_reads() -> Result<()> {
    // Arrange
    let world = World::build().await?;
    let heap_endpoints = endpoints()
        .into_iter()
        .filter(|e| matches!(e.access, Access::Heap { .. }));
    let mut mismatches = Vec::new();

    // Act
    for endpoint in heap_endpoints {
        for caller in [Caller::Member, Caller::Admin] {
            let response = world.call(&endpoint, caller, true).await?;
            let (got, want) = (Outcome::of(&response), expected_outcome(&endpoint, caller, true));
            if got != want {
                mismatches.push(format!(
                    "{} {} as {caller:?}: got {got:?}, want {want:?}",
                    endpoint.method, endpoint.path
                ));
            }
        }
    }

    // Assert
    expect_that!(mismatches, is_empty());
    Ok(())
}

#[gtest]
#[test]
fn every_endpoint_is_in_the_table() -> Result<()> {
    // Arrange
    let spec = serde_json::to_value(api::openapi())?;
    let paths = spec["paths"].as_object().context("spec has no paths")?;
    let documented: BTreeSet<String> = paths
        .iter()
        .flat_map(|(path, item)| {
            let methods = item
                .as_object()
                .map(|m| m.keys().cloned().collect::<Vec<_>>())
                .unwrap_or_default();
            methods
                .into_iter()
                .map(move |method| format!("{} {path}", method.to_uppercase()))
        })
        .filter(|route| !route.starts_with("PARAMETERS "))
        .collect();

    // Act
    let tabled: BTreeSet<String> = endpoints()
        .iter()
        .map(|e| format!("{} /api/v1{}", e.method, e.path))
        .collect();

    // Assert
    expect_that!(
        documented.difference(&tabled).collect::<Vec<_>>(),
        is_empty(),
        "endpoints missing from the table"
    );
    expect_that!(
        tabled.difference(&documented).collect::<Vec<_>>(),
        is_empty(),
        "table rows with no endpoint"
    );
    Ok(())
}
