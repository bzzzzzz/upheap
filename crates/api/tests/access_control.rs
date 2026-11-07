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
    /// Members of the team holding `permission`.
    Team { permission: Permission },
}

/// What a request needs to exist before it is sent.
#[derive(Clone, Copy)]
enum Target {
    Nothing,
    Heap,
    /// The shared heap, with its version for `If-Match`.
    HeapVersion,
    /// A heap of its own, so archiving it leaves the shared heap alone.
    FreshHeap,
    OpenItem,
    ClosedItem,
    Tag,
    /// A saved view of the shared heap, with its version for `If-Match`.
    SavedView,
    /// A parameter of the shared heap with two options, with its version for `If-Match`.
    Parameter,
    /// The first option of a new parameter of the shared heap.
    ParameterOption,
    /// A session of its own, so signing out doesn't end the caller's shared session.
    FreshSession,
    /// The shared team, with its version for `If-Match`.
    Team,
    /// A new user, added to the shared team as a member, in `{user_id}`.
    ExtraMember,
    /// A new user who isn't in the shared team yet, for the request body.
    NewUser,
    /// A team of its own with a second admin, which the admin and the member of the shared team also belong to, so
    /// they can leave it without leaving the shared team or being its last admin.
    FreshTeam,
    /// The caller's own user, with its version for `If-Match`.
    Me,
    /// A lookup text in the query string.
    UserQuery,
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
    /// A saved view that shows blocked items.
    NewView,
    /// A team, heap or saved view name.
    Name,
    /// A parameter name no other request used, since names are unique per heap.
    NewParameter,
    /// A label for a new option.
    NewOption,
    /// The target parameter's options, in reverse order.
    ArrangeOptions,
    /// The target option's replacement: the other option of its parameter.
    ArchiveOption,
    /// No tags.
    LinkTags,
    /// The user from [`Target::NewUser`].
    AddMember,
    Promote,
    DisplayName,
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
    use Access::{Heap, Public, SignedIn, Team};
    use Permission::{ConfigureHeap, EditItems, LeaveTeam, ManageHeaps, ManageTeam, ViewHeap, ViewTeam};
    let view = Heap {
        permission: ViewHeap,
        writes: false,
    };
    let edit = Heap {
        permission: EditItems,
        writes: true,
    };
    let manage_heap = |writes| Heap {
        permission: ManageHeaps,
        writes,
    };
    let configure = Heap {
        permission: ConfigureHeap,
        writes: true,
    };
    let team = |permission| Team { permission };
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
            Method::PATCH,
            "/me",
            SignedIn,
            Target::Me,
            RequestBody::DisplayName,
            StatusCode::OK,
        ),
        // Every user is the admin of their personal team, so every signed-in caller here may look users up. The
        // refusal for a user who is no team's admin is tested in `teams.rs`.
        row(
            Method::GET,
            "/users",
            SignedIn,
            Target::UserQuery,
            RequestBody::Empty,
            StatusCode::OK,
        ),
        row(
            Method::POST,
            "/teams",
            SignedIn,
            Target::Nothing,
            RequestBody::Name,
            StatusCode::CREATED,
        ),
        row(
            Method::GET,
            "/teams/{team_id}",
            team(ViewTeam),
            Target::Nothing,
            RequestBody::Empty,
            StatusCode::OK,
        ),
        row(
            Method::GET,
            "/teams/{team_id}/audit",
            team(ViewTeam),
            Target::Nothing,
            RequestBody::Empty,
            StatusCode::OK,
        ),
        row(
            Method::PATCH,
            "/teams/{team_id}",
            team(ManageTeam),
            Target::Team,
            RequestBody::Name,
            StatusCode::OK,
        ),
        row(
            Method::POST,
            "/teams/{team_id}/members",
            team(ManageTeam),
            Target::NewUser,
            RequestBody::AddMember,
            StatusCode::CREATED,
        ),
        row(
            Method::PATCH,
            "/teams/{team_id}/members/{user_id}",
            team(ManageTeam),
            Target::ExtraMember,
            RequestBody::Promote,
            StatusCode::OK,
        ),
        row(
            Method::DELETE,
            "/teams/{team_id}/members/{user_id}",
            team(ManageTeam),
            Target::ExtraMember,
            RequestBody::Empty,
            StatusCode::NO_CONTENT,
        ),
        row(
            Method::POST,
            "/teams/{team_id}/leave",
            team(LeaveTeam),
            Target::FreshTeam,
            RequestBody::Empty,
            StatusCode::NO_CONTENT,
        ),
        row(
            Method::POST,
            "/teams/{team_id}/heaps",
            team(ManageHeaps),
            Target::Nothing,
            RequestBody::Name,
            StatusCode::CREATED,
        ),
        row(
            Method::GET,
            "/heaps/{heap_id}",
            view,
            Target::Heap,
            RequestBody::Empty,
            StatusCode::OK,
        ),
        row(
            Method::PATCH,
            "/heaps/{heap_id}",
            manage_heap(true),
            Target::HeapVersion,
            RequestBody::Name,
            StatusCode::OK,
        ),
        // Archiving and unarchiving say what the heap should end up as, so they succeed on an archived heap too.
        row(
            Method::POST,
            "/heaps/{heap_id}/archive",
            manage_heap(false),
            Target::FreshHeap,
            RequestBody::Empty,
            StatusCode::OK,
        ),
        row(
            Method::POST,
            "/heaps/{heap_id}/unarchive",
            manage_heap(false),
            Target::FreshHeap,
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
            Method::GET,
            "/heaps/{heap_id}/audit",
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
        row(
            Method::GET,
            "/heaps/{heap_id}/views",
            view,
            Target::Heap,
            RequestBody::Empty,
            StatusCode::OK,
        ),
        row(
            Method::POST,
            "/heaps/{heap_id}/views",
            edit,
            Target::Heap,
            RequestBody::NewView,
            StatusCode::CREATED,
        ),
        row(
            Method::PATCH,
            "/views/{view_id}",
            edit,
            Target::SavedView,
            RequestBody::Name,
            StatusCode::OK,
        ),
        row(
            Method::POST,
            "/views/{view_id}/archive",
            edit,
            Target::SavedView,
            RequestBody::Empty,
            StatusCode::OK,
        ),
        row(
            Method::GET,
            "/heaps/{heap_id}/parameters",
            view,
            Target::Heap,
            RequestBody::Empty,
            StatusCode::OK,
        ),
        row(
            Method::POST,
            "/heaps/{heap_id}/parameters",
            configure,
            Target::Heap,
            RequestBody::NewParameter,
            StatusCode::CREATED,
        ),
        row(
            Method::PATCH,
            "/parameters/{parameter_id}",
            configure,
            Target::Parameter,
            RequestBody::NewParameter,
            StatusCode::OK,
        ),
        row(
            Method::POST,
            "/parameters/{parameter_id}/archive",
            configure,
            Target::Parameter,
            RequestBody::Empty,
            StatusCode::OK,
        ),
        row(
            Method::POST,
            "/parameters/{parameter_id}/options",
            configure,
            Target::Parameter,
            RequestBody::NewOption,
            StatusCode::OK,
        ),
        row(
            Method::PATCH,
            "/parameters/{parameter_id}/options",
            configure,
            Target::Parameter,
            RequestBody::ArrangeOptions,
            StatusCode::OK,
        ),
        row(
            Method::PUT,
            "/parameters/{parameter_id}/tags",
            configure,
            Target::Parameter,
            RequestBody::LinkTags,
            StatusCode::OK,
        ),
        row(
            Method::POST,
            "/options/{option_id}/archive",
            configure,
            Target::ParameterOption,
            RequestBody::ArchiveOption,
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
        Access::SignedIn | Access::Heap { .. } | Access::Team { .. } if matches!(caller, Caller::Anonymous) => {
            return Outcome::problem(StatusCode::UNAUTHORIZED, "unauthenticated");
        }
        Access::SignedIn => return Outcome::success(endpoint.success),
        Access::Heap { permission, writes } => (permission, writes),
        Access::Team { permission } => (permission, false),
    };
    let role = match caller {
        Caller::Anonymous => return Outcome::problem(StatusCode::UNAUTHORIZED, "unauthenticated"),
        // Non-members can't tell a heap or team they may not see from one that doesn't exist.
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
    team: String,
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
        let team = admin_me.body["teams"][0]["id"]
            .as_str()
            .context("admin has no team")?
            .to_owned();
        let member_id = user_id(&app, &member_session).await?;
        app.add_member(&team, &member_id, Role::Member).await?;

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
            team,
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

    /// Saves a view of the shared heap as the admin and returns its ID and version.
    async fn create_saved_view(&self) -> Result<(String, String)> {
        let created = self
            .app
            .send(
                Method::POST,
                &format!("/api/v1/heaps/{}/views", self.heap),
                Some(&self.admin.session),
                Some(json!({ "name": "Target" })),
            )
            .await?;
        let id = created.body["id"].as_str().context("view has no id")?.to_owned();
        Ok((id, created.body["version"].to_string()))
    }

    /// Creates a parameter with two options as the admin and returns its body.
    async fn create_parameter(&self) -> Result<Value> {
        let created = self
            .app
            .send(
                Method::POST,
                &format!("/api/v1/heaps/{}/parameters", self.heap),
                Some(&self.admin.session),
                Some(json!({ "name": self.unique_name("target"), "options": ["High", "Low"] })),
            )
            .await?;
        anyhow::ensure!(
            created.status == StatusCode::CREATED,
            "create parameter failed: {}",
            created.body
        );
        Ok(created.body)
    }

    /// Registers a new user and returns their session and ID.
    async fn new_user(&self) -> Result<(String, String)> {
        let email = format!("{}@example.test", self.unique_name("user"));
        let (session, _) = self.app.register(&email).await?;
        let id = user_id(&self.app, &session).await?;
        Ok((session, id))
    }

    /// Adds a new user to the shared team as a member and returns their ID.
    async fn new_member(&self) -> Result<String> {
        let (_, id) = self.new_user().await?;
        self.app.add_member(&self.team, &id, Role::Member).await?;
        Ok(id)
    }

    /// A new team whose admins are a new user and the shared team's admin, with the shared team's member in it.
    async fn new_team(&self) -> Result<String> {
        let (owner, _) = self.new_user().await?;
        let created = self
            .app
            .send(
                Method::POST,
                "/api/v1/teams",
                Some(&owner),
                Some(json!({ "name": "Fresh" })),
            )
            .await?;
        let team = created.body["id"].as_str().context("team has no id")?.to_owned();
        let admin_id = user_id(&self.app, &self.admin.session).await?;
        let member_id = user_id(&self.app, &self.member.session).await?;
        self.app.add_member(&team, &admin_id, Role::Admin).await?;
        self.app.add_member(&team, &member_id, Role::Member).await?;
        Ok(team)
    }

    /// A new heap in the shared team, created by its admin.
    async fn new_heap(&self) -> Result<String> {
        let created = self
            .app
            .send(
                Method::POST,
                &format!("/api/v1/teams/{}/heaps", self.team),
                Some(&self.admin.session),
                Some(json!({ "name": "Fresh" })),
            )
            .await?;
        Ok(created.body["id"].as_str().context("heap has no id")?.to_owned())
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

    /// The body for `body`. `user` is the user [`Target::NewUser`] made, and `parameter` the parameter
    /// [`Target::Parameter`] or [`Target::ParameterOption`] made.
    fn body(&self, body: RequestBody, user: Option<&str>, parameter: &Value) -> Option<Value> {
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
            RequestBody::NewView => Some(json!({ "name": "Blocked", "view": { "blocked": "shown" } })),
            RequestBody::Name => Some(json!({ "name": self.unique_name("name") })),
            RequestBody::AddMember => Some(json!({ "user_id": user })),
            RequestBody::Promote => Some(json!({ "role": "admin" })),
            RequestBody::DisplayName => Some(json!({ "display_name": self.unique_name("Name") })),
            RequestBody::NewParameter => Some(json!({ "name": self.unique_name("parameter") })),
            RequestBody::NewOption => Some(json!({ "label": "Middle" })),
            RequestBody::ArrangeOptions => {
                let mut options = parameter["options"].as_array().cloned().unwrap_or_default();
                options.reverse();
                Some(json!({ "options": options }))
            }
            RequestBody::ArchiveOption => Some(json!({ "replacement_option_id": parameter["options"][1]["id"] })),
            RequestBody::LinkTags => Some(json!({ "tag_ids": [] })),
        }
    }

    /// Prepares what `endpoint` needs, then calls it as `caller`, with the heap archived for the call when asked.
    async fn call(&self, endpoint: &Endpoint, caller: Caller, archived: bool) -> Result<TestResponse> {
        let account = self.account(caller);
        let mut session = account.map(|a| a.session.clone());
        let mut path = endpoint
            .path
            .replace("{heap_id}", &self.heap)
            .replace("{team_id}", &self.team);
        let mut if_match = None;
        let mut new_user = None;
        let mut parameter = Value::Null;
        match endpoint.target {
            Target::Nothing | Target::Heap => {}
            Target::HeapVersion => {
                let heap = self
                    .app
                    .send(
                        Method::GET,
                        &format!("/api/v1/heaps/{}", self.heap),
                        Some(&self.admin.session),
                        None,
                    )
                    .await?;
                if_match = Some(heap.body["version"].to_string());
            }
            Target::FreshHeap => path = endpoint.path.replace("{heap_id}", &self.new_heap().await?),
            Target::Team => {
                let team = self
                    .app
                    .send(
                        Method::GET,
                        &format!("/api/v1/teams/{}", self.team),
                        Some(&self.admin.session),
                        None,
                    )
                    .await?;
                if_match = Some(team.body["version"].to_string());
            }
            Target::ExtraMember => path = path.replace("{user_id}", &self.new_member().await?),
            Target::NewUser => new_user = Some(self.new_user().await?.1),
            Target::FreshTeam => path = endpoint.path.replace("{team_id}", &self.new_team().await?),
            Target::Me => {
                if let Some(session) = &session {
                    let me = self.app.send(Method::GET, "/api/v1/me", Some(session), None).await?;
                    if_match = Some(me.body["user"]["version"].to_string());
                }
            }
            Target::UserQuery => path.push_str("?query=example"),
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
            Target::SavedView => {
                let (id, version) = self.create_saved_view().await?;
                path = path.replace("{view_id}", &id);
                if_match = Some(version);
            }
            Target::Parameter => {
                parameter = self.create_parameter().await?;
                let id = parameter["id"].as_str().context("parameter has no id")?;
                path = path.replace("{parameter_id}", id);
                if_match = Some(parameter["version"].to_string());
            }
            Target::ParameterOption => {
                parameter = self.create_parameter().await?;
                let id = parameter["options"][0]["id"]
                    .as_str()
                    .context("parameter has no option")?;
                path = path.replace("{option_id}", id);
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
                self.body(endpoint.body, new_user.as_deref(), &parameter),
                &headers,
            )
            .await;
        if archived {
            self.app.set_heap_archived(&self.heap, false).await?;
        }
        response
    }
}

async fn user_id(app: &TestApp, session: &str) -> Result<String> {
    let me = app.send(Method::GET, "/api/v1/me", Some(session), None).await?;
    Ok(me.body["user"]["id"].as_str().context("user has no id")?.to_owned())
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
