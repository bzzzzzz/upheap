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
    /// Callers holding `permission` in some shared team. The outsider holds it only in their personal team.
    SharedTeam { permission: Permission },
    /// A run of the caller's own. Anyone signed in may call it, but a run belongs to the user who claimed it, so
    /// the outsider, who can't claim in the shared heap, finds none.
    OwnRun,
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
    /// An open item in the shared heap, and a second heap of the team to move it to, in the request's body.
    ItemAndSecondHeap,
    /// A task with one step, in `{item}` and `{step}`, with the step's version for `If-Match`.
    TaskWithStep,
    /// An item with one open question, in `{item}` and `{question}`, with the question's version for `If-Match`.
    ItemWithQuestion,
    Tag,
    /// A saved view of the shared heap, with its version for `If-Match`.
    SavedView,
    /// A parameter of the shared heap with two options, with its version for `If-Match`.
    Parameter,
    /// The first option of a new parameter of the shared heap.
    ParameterOption,
    /// A session of its own, so signing out doesn't end the caller's shared session.
    FreshSession,
    /// An account of its own, signed in when the caller is, so a request that replaces the caller's session (a
    /// password change) doesn't end the caller's shared one.
    OwnAccount,
    /// The shared team, with its version for `If-Match`.
    Team,
    /// A new user, added to the shared team as a member, in `{username}`.
    ExtraMember,
    /// A new user who isn't in the shared team yet, for the request body.
    NewUser,
    /// A team of its own with a second admin, which the admin and the member of the shared team also belong to, so
    /// they can leave it without leaving the shared team or being its last admin.
    FreshTeam,
    /// A working invite link of the shared team, made by its admin, in `{id}`.
    InviteLink,
    /// A working invite link of a team of its own, made by a new admin none of the callers know, so every caller is a
    /// newcomer to it and joining leaves the shared team alone. Its token is in `{token}`.
    ForeignInviteLink,
    /// A working password reset link of an account of its own, in `{token}`, so resetting it ends none of the callers'
    /// sign-ins.
    ResetLink,
    /// The caller's own user, with its version for `If-Match`.
    Me,
    /// A sign-in of the CLI as the caller, in `{id}`.
    SignIn,
    /// A lookup text in the query string.
    UserQuery,
    /// The CLI's authorization request in the query string.
    AuthorizationQuery,
    /// An authorization code the shared team's admin approved for the CLI.
    AuthorizationCode,
    /// The shared heap in the query string, as the agent's queue takes its heaps.
    Queue,
    /// An item with an open turn in the shared heap, so the queue has something to claim.
    Claimable,
    /// An item whose turn is stuck, in `{item}`.
    StuckItem,
    /// A run the caller claimed on an item that has a context, in `{run}`. A caller who can't claim gets a run ID
    /// nobody has.
    OwnRun,
}

/// The body a request carries.
#[derive(Clone, Copy)]
enum RequestBody {
    Empty,
    Registration,
    SignIn,
    NewItem,
    Rename,
    /// The second heap of [`Target::ItemAndSecondHeap`], to move the item to.
    MoveItem,
    /// A step to add.
    NewStep,
    /// A step ticked.
    TickStep,
    /// A question to ask.
    NewQuestion,
    /// The reply to the target question, which has no choices.
    ReplyToQuestion,
    /// Sending an item to an agent, one state.
    SendToAgent,
    /// A claim over the shared heap.
    Claim,
    /// A run reaching its goal.
    Reached,
    /// A run finishing.
    Finish,
    /// A tag name no other request used, since names are unique per heap.
    NewTag,
    RenameTag,
    /// A saved view that shows blocked items.
    NewView,
    /// A team, heap or saved view name.
    Name,
    /// A new context for the heap.
    HeapContext,
    /// A new need weight for the heap.
    Weights,
    /// A ranking preview with the stored weights.
    RankingPreview,
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
    /// The password of the account from [`Target::OwnAccount`], as both the current and the new one.
    ChangePassword,
    /// The new password for the account of [`Target::ResetLink`].
    NewPassword,
    /// The CLI's authorization request.
    Authorization,
    /// A form exchanging the code from [`Target::AuthorizationCode`], as the CLI sends it, without cookie or CSRF
    /// header.
    TokenExchange,
    /// A form revoking a token, as the CLI sends it.
    Revocation,
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
    use Access::{Heap, Public, SharedTeam, SignedIn, Team};
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
            "/oauth/authorize",
            Public,
            Target::AuthorizationQuery,
            RequestBody::Empty,
            StatusCode::OK,
        ),
        row(
            Method::POST,
            "/oauth/authorize",
            SignedIn,
            Target::Nothing,
            RequestBody::Authorization,
            StatusCode::OK,
        ),
        // The token endpoints take no cookie: whoever holds the code or token may use it.
        row(
            Method::POST,
            "/oauth/token",
            Public,
            Target::AuthorizationCode,
            RequestBody::TokenExchange,
            StatusCode::OK,
        ),
        row(
            Method::POST,
            "/oauth/revoke",
            Public,
            Target::Nothing,
            RequestBody::Revocation,
            StatusCode::OK,
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
        row(
            Method::GET,
            "/me/waiting",
            SignedIn,
            Target::Nothing,
            RequestBody::Empty,
            StatusCode::OK,
        ),
        row(
            Method::GET,
            "/me/items",
            SignedIn,
            Target::Nothing,
            RequestBody::Empty,
            StatusCode::OK,
        ),
        row(
            Method::GET,
            "/me/sign-ins",
            SignedIn,
            Target::Nothing,
            RequestBody::Empty,
            StatusCode::OK,
        ),
        row(
            Method::DELETE,
            "/me/sign-ins/{id}",
            SignedIn,
            Target::SignIn,
            RequestBody::Empty,
            StatusCode::NO_CONTENT,
        ),
        row(
            Method::POST,
            "/me/password",
            SignedIn,
            Target::OwnAccount,
            RequestBody::ChangePassword,
            StatusCode::OK,
        ),
        row(
            Method::GET,
            "/me/history",
            SignedIn,
            Target::OwnAccount,
            RequestBody::Empty,
            StatusCode::OK,
        ),
        row(
            Method::GET,
            "/password-resets/{token}",
            Public,
            Target::ResetLink,
            RequestBody::Empty,
            StatusCode::OK,
        ),
        row(
            Method::POST,
            "/password-resets/{token}",
            Public,
            Target::ResetLink,
            RequestBody::NewPassword,
            StatusCode::OK,
        ),
        row(
            Method::GET,
            "/due-choices",
            SignedIn,
            Target::Nothing,
            RequestBody::Empty,
            StatusCode::OK,
        ),
        // Every user is the admin of their personal team, so every signed-in caller here may look users up. The
        // refusal for a user who is no team's admin is tested in `teams.rs`.
        row(
            Method::GET,
            "/users",
            SharedTeam {
                permission: Permission::ManageTeam,
            },
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
            "/teams/{team}",
            team(ViewTeam),
            Target::Nothing,
            RequestBody::Empty,
            StatusCode::OK,
        ),
        row(
            Method::GET,
            "/teams/{team}/audit",
            team(ViewTeam),
            Target::Nothing,
            RequestBody::Empty,
            StatusCode::OK,
        ),
        row(
            Method::PATCH,
            "/teams/{team}",
            team(ManageTeam),
            Target::Team,
            RequestBody::Name,
            StatusCode::OK,
        ),
        row(
            Method::POST,
            "/teams/{team}/members",
            team(ManageTeam),
            Target::NewUser,
            RequestBody::AddMember,
            StatusCode::CREATED,
        ),
        row(
            Method::PATCH,
            "/teams/{team}/members/{username}",
            team(ManageTeam),
            Target::ExtraMember,
            RequestBody::Promote,
            StatusCode::OK,
        ),
        row(
            Method::DELETE,
            "/teams/{team}/members/{username}",
            team(ManageTeam),
            Target::ExtraMember,
            RequestBody::Empty,
            StatusCode::NO_CONTENT,
        ),
        row(
            Method::POST,
            "/teams/{team}/leave",
            team(LeaveTeam),
            Target::FreshTeam,
            RequestBody::Empty,
            StatusCode::NO_CONTENT,
        ),
        row(
            Method::GET,
            "/teams/{team}/invite-links",
            team(ManageTeam),
            Target::Nothing,
            RequestBody::Empty,
            StatusCode::OK,
        ),
        row(
            Method::POST,
            "/teams/{team}/invite-links",
            team(ManageTeam),
            Target::Nothing,
            RequestBody::Empty,
            StatusCode::CREATED,
        ),
        row(
            Method::POST,
            "/teams/{team}/invite-links/{id}/revoke",
            team(ManageTeam),
            Target::InviteLink,
            RequestBody::Empty,
            StatusCode::OK,
        ),
        row(
            Method::GET,
            "/invites/{token}",
            Public,
            Target::ForeignInviteLink,
            RequestBody::Empty,
            StatusCode::OK,
        ),
        row(
            Method::POST,
            "/invites/{token}/join",
            SignedIn,
            Target::ForeignInviteLink,
            RequestBody::Empty,
            StatusCode::CREATED,
        ),
        row(
            Method::POST,
            "/invites/{token}/register",
            Public,
            Target::ForeignInviteLink,
            RequestBody::Registration,
            StatusCode::CREATED,
        ),
        row(
            Method::POST,
            "/teams/{team}/heaps",
            team(ManageHeaps),
            Target::Nothing,
            RequestBody::Name,
            StatusCode::CREATED,
        ),
        row(
            Method::GET,
            "/teams/{team}/heaps/{heap}",
            view,
            Target::Heap,
            RequestBody::Empty,
            StatusCode::OK,
        ),
        row(
            Method::PATCH,
            "/teams/{team}/heaps/{heap}",
            manage_heap(true),
            Target::HeapVersion,
            RequestBody::Name,
            StatusCode::OK,
        ),
        // The same route again: a context needs only EditItems, a name needs ManageHeaps.
        row(
            Method::PATCH,
            "/teams/{team}/heaps/{heap}",
            edit,
            Target::HeapVersion,
            RequestBody::HeapContext,
            StatusCode::OK,
        ),
        row(
            Method::PATCH,
            "/teams/{team}/heaps/{heap}/weights",
            configure,
            Target::HeapVersion,
            RequestBody::Weights,
            StatusCode::OK,
        ),
        // A read that carries a body, so it works on an archived heap too.
        row(
            Method::POST,
            "/teams/{team}/heaps/{heap}/ranking/preview",
            view,
            Target::Heap,
            RequestBody::RankingPreview,
            StatusCode::OK,
        ),
        // Archiving and unarchiving say what the heap should end up as, so they succeed on an archived heap too.
        row(
            Method::POST,
            "/teams/{team}/heaps/{heap}/archive",
            manage_heap(false),
            Target::FreshHeap,
            RequestBody::Empty,
            StatusCode::OK,
        ),
        row(
            Method::POST,
            "/teams/{team}/heaps/{heap}/unarchive",
            manage_heap(false),
            Target::FreshHeap,
            RequestBody::Empty,
            StatusCode::OK,
        ),
        row(
            Method::GET,
            "/teams/{team}/heaps/{heap}/items",
            view,
            Target::Heap,
            RequestBody::Empty,
            StatusCode::OK,
        ),
        row(
            Method::GET,
            "/teams/{team}/heaps/{heap}/items/closed",
            view,
            Target::Heap,
            RequestBody::Empty,
            StatusCode::OK,
        ),
        row(
            Method::GET,
            "/teams/{team}/heaps/{heap}/audit",
            view,
            Target::Heap,
            RequestBody::Empty,
            StatusCode::OK,
        ),
        row(
            Method::POST,
            "/teams/{team}/heaps/{heap}/items",
            edit,
            Target::Heap,
            RequestBody::NewItem,
            StatusCode::CREATED,
        ),
        row(
            Method::GET,
            "/teams/{team}/heaps/{heap}/items/{item}",
            view,
            Target::OpenItem,
            RequestBody::Empty,
            StatusCode::OK,
        ),
        row(
            Method::PATCH,
            "/teams/{team}/heaps/{heap}/items/{item}",
            edit,
            Target::OpenItem,
            RequestBody::Rename,
            StatusCode::OK,
        ),
        row(
            Method::POST,
            "/teams/{team}/heaps/{heap}/items/{item}/steps",
            edit,
            Target::TaskWithStep,
            RequestBody::NewStep,
            StatusCode::CREATED,
        ),
        row(
            Method::PATCH,
            "/teams/{team}/heaps/{heap}/items/{item}/steps/{step}",
            edit,
            Target::TaskWithStep,
            RequestBody::TickStep,
            StatusCode::OK,
        ),
        row(
            Method::POST,
            "/teams/{team}/heaps/{heap}/items/{item}/questions",
            edit,
            Target::OpenItem,
            RequestBody::NewQuestion,
            StatusCode::CREATED,
        ),
        row(
            Method::PATCH,
            "/teams/{team}/heaps/{heap}/items/{item}/questions/{question}",
            edit,
            Target::ItemWithQuestion,
            RequestBody::ReplyToQuestion,
            StatusCode::OK,
        ),
        row(
            Method::POST,
            "/teams/{team}/heaps/{heap}/items/{item}/turn",
            edit,
            Target::OpenItem,
            RequestBody::SendToAgent,
            StatusCode::CREATED,
        ),
        row(
            Method::DELETE,
            "/teams/{team}/heaps/{heap}/items/{item}/turn",
            edit,
            Target::OpenItem,
            RequestBody::Empty,
            StatusCode::OK,
        ),
        row(
            Method::POST,
            "/teams/{team}/heaps/{heap}/items/{item}/turn/retry",
            edit,
            Target::StuckItem,
            RequestBody::Empty,
            StatusCode::OK,
        ),
        // The queue refuses an archived heap even to read, as it is for claiming.
        row(
            Method::GET,
            "/work/next",
            edit,
            Target::Queue,
            RequestBody::Empty,
            StatusCode::OK,
        ),
        row(
            Method::POST,
            "/work/runs",
            edit,
            Target::Claimable,
            RequestBody::Claim,
            StatusCode::CREATED,
        ),
        row(
            Method::POST,
            "/work/runs/{run}/heartbeat",
            Access::OwnRun,
            Target::OwnRun,
            RequestBody::Empty,
            StatusCode::NO_CONTENT,
        ),
        row(
            Method::POST,
            "/work/runs/{run}/reached",
            Access::OwnRun,
            Target::OwnRun,
            RequestBody::Reached,
            StatusCode::OK,
        ),
        row(
            Method::POST,
            "/work/runs/{run}/finish",
            Access::OwnRun,
            Target::OwnRun,
            RequestBody::Finish,
            StatusCode::OK,
        ),
        row(
            Method::POST,
            "/teams/{team}/heaps/{heap}/items/{item}/reopen",
            edit,
            Target::ClosedItem,
            RequestBody::Empty,
            StatusCode::OK,
        ),
        row(
            Method::POST,
            "/teams/{team}/heaps/{heap}/items/{item}/move",
            edit,
            Target::ItemAndSecondHeap,
            RequestBody::MoveItem,
            StatusCode::OK,
        ),
        row(
            Method::GET,
            "/teams/{team}/heaps/{heap}/tags",
            view,
            Target::Heap,
            RequestBody::Empty,
            StatusCode::OK,
        ),
        row(
            Method::POST,
            "/teams/{team}/heaps/{heap}/tags",
            edit,
            Target::Heap,
            RequestBody::NewTag,
            StatusCode::CREATED,
        ),
        row(
            Method::PATCH,
            "/teams/{team}/heaps/{heap}/tags/{tag_id}",
            edit,
            Target::Tag,
            RequestBody::RenameTag,
            StatusCode::OK,
        ),
        row(
            Method::GET,
            "/teams/{team}/heaps/{heap}/views",
            view,
            Target::Heap,
            RequestBody::Empty,
            StatusCode::OK,
        ),
        row(
            Method::POST,
            "/teams/{team}/heaps/{heap}/views",
            edit,
            Target::Heap,
            RequestBody::NewView,
            StatusCode::CREATED,
        ),
        row(
            Method::PATCH,
            "/teams/{team}/heaps/{heap}/views/{view_id}",
            edit,
            Target::SavedView,
            RequestBody::Name,
            StatusCode::OK,
        ),
        row(
            Method::POST,
            "/teams/{team}/heaps/{heap}/views/{view_id}/archive",
            edit,
            Target::SavedView,
            RequestBody::Empty,
            StatusCode::OK,
        ),
        row(
            Method::GET,
            "/teams/{team}/heaps/{heap}/parameters",
            view,
            Target::Heap,
            RequestBody::Empty,
            StatusCode::OK,
        ),
        row(
            Method::POST,
            "/teams/{team}/heaps/{heap}/parameters",
            configure,
            Target::Heap,
            RequestBody::NewParameter,
            StatusCode::CREATED,
        ),
        row(
            Method::PATCH,
            "/teams/{team}/heaps/{heap}/parameters/{parameter_id}",
            configure,
            Target::Parameter,
            RequestBody::NewParameter,
            StatusCode::OK,
        ),
        row(
            Method::POST,
            "/teams/{team}/heaps/{heap}/parameters/{parameter_id}/archive",
            configure,
            Target::Parameter,
            RequestBody::Empty,
            StatusCode::OK,
        ),
        row(
            Method::POST,
            "/teams/{team}/heaps/{heap}/parameters/{parameter_id}/options",
            configure,
            Target::Parameter,
            RequestBody::NewOption,
            StatusCode::OK,
        ),
        row(
            Method::PATCH,
            "/teams/{team}/heaps/{heap}/parameters/{parameter_id}/options",
            configure,
            Target::Parameter,
            RequestBody::ArrangeOptions,
            StatusCode::OK,
        ),
        row(
            Method::PUT,
            "/teams/{team}/heaps/{heap}/parameters/{parameter_id}/tags",
            configure,
            Target::Parameter,
            RequestBody::LinkTags,
            StatusCode::OK,
        ),
        row(
            Method::POST,
            "/teams/{team}/heaps/{heap}/options/{option_id}/archive",
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
        Access::SignedIn | Access::Heap { .. } | Access::Team { .. } | Access::SharedTeam { .. } | Access::OwnRun
            if matches!(caller, Caller::Anonymous) =>
        {
            return Outcome::problem(StatusCode::UNAUTHORIZED, "unauthenticated");
        }
        Access::SignedIn => return Outcome::success(endpoint.success),
        Access::OwnRun => {
            return if matches!(caller, Caller::Outsider) {
                Outcome::problem(StatusCode::NOT_FOUND, "not_found")
            } else {
                Outcome::success(endpoint.success)
            };
        }
        Access::Heap { permission, writes } => (permission, writes),
        Access::Team { permission } => (permission, false),
        Access::SharedTeam { permission } => {
            if matches!(caller, Caller::Outsider) {
                return Outcome::problem(StatusCode::FORBIDDEN, "forbidden");
            }
            (permission, false)
        }
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
    /// The shared team's and heap's slugs, for `{team}` and `{heap}`.
    team_slug: String,
    heap_slug: String,
    /// Counter for names that must be unique: emails and tag names.
    unique_names: AtomicU32,
}

impl World {
    async fn build() -> Result<Self> {
        let app = TestApp::start().await?;
        let (admin_session, _) = app.register("admin@example.test").await?;
        let (member_session, _) = app.register("member@example.test").await?;
        let (outsider_session, _) = app.register("outsider@example.test").await?;

        // A personal team takes no members, so the shared team is one of its own.
        let created = app
            .send(
                Method::POST,
                "/api/v1/teams",
                Some(&admin_session),
                Some(json!({ "name": "Shared" })),
            )
            .await?;
        let team = created.body["id"].as_str().context("team has no id")?.to_owned();
        let team_slug = created.body["slug"].as_str().context("team has no slug")?.to_owned();
        let created = app
            .send(
                Method::POST,
                &format!("/api/v1/teams/{team_slug}/heaps"),
                Some(&admin_session),
                Some(json!({ "name": "Shared" })),
            )
            .await?;
        let heap = created.body["id"].as_str().context("heap has no id")?.to_owned();
        let heap_slug = created.body["slug"].as_str().context("heap has no slug")?.to_owned();
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
            team_slug,
            heap_slug,
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

    /// The shared heap's path under `/api/v1`.
    fn heap_path(&self) -> String {
        format!("/api/v1/teams/{}/heaps/{}", self.team_slug, self.heap_slug)
    }

    /// Creates an item as the admin, closing it when `closed`, and returns its key and version.
    async fn create_item(&self, closed: bool) -> Result<(String, String)> {
        let created = self
            .app
            .send(
                Method::POST,
                &format!("{}/items", self.heap_path()),
                Some(&self.admin.session),
                Some(json!({ "need": "expected", "title": "Target" })),
            )
            .await?;
        let key = created.body["key"].as_str().context("item has no key")?.to_owned();
        if !closed {
            return Ok((key, created.body["version"].to_string()));
        }
        let done = self
            .app
            .send_with(
                Method::PATCH,
                &format!("{}/items/{key}", self.heap_path()),
                Some(&self.admin.session),
                Some(json!({ "state": "done" })),
                &[
                    ("x-csrf-protection", "1"),
                    ("if-match", &created.body["version"].to_string()),
                ],
            )
            .await?;
        Ok((key, done.body["version"].to_string()))
    }

    /// Creates a task with one step as the admin and returns the task's key and the step's version.
    async fn create_task_with_step(&self) -> Result<(String, String)> {
        let (key, version) = self.create_item(false).await?;
        let item_path = format!("{}/items/{key}", self.heap_path());
        let admin = &self.admin.session;
        let task = self
            .app
            .patch(admin, &item_path, &json!(version), json!({ "kind": "task" }))
            .await?;
        anyhow::ensure!(task.status == StatusCode::OK, "make task failed: {}", task.body);
        let added = self
            .app
            .send(
                Method::POST,
                &format!("{item_path}/steps"),
                Some(admin),
                Some(json!({ "title": "Target step" })),
            )
            .await?;
        anyhow::ensure!(added.status == StatusCode::CREATED, "add step failed: {}", added.body);
        Ok((key, added.body["steps"][0]["version"].to_string()))
    }

    /// Creates an item with one open question as the admin and returns the item's key and the question's version.
    async fn create_item_with_question(&self) -> Result<(String, String)> {
        let (key, _) = self.create_item(false).await?;
        let asked = self
            .app
            .send(
                Method::POST,
                &format!("{}/items/{key}/questions", self.heap_path()),
                Some(&self.admin.session),
                Some(json!({ "kind": "question", "text": "Which CA?" })),
            )
            .await?;
        anyhow::ensure!(asked.status == StatusCode::CREATED, "ask failed: {}", asked.body);
        Ok((key, asked.body["questions"][0]["version"].to_string()))
    }

    /// Creates an item as the admin with a context, and sends it to an agent. Returns its key.
    async fn create_item_with_turn(&self, need: &str) -> Result<String> {
        let created = self
            .app
            .send(
                Method::POST,
                &format!("{}/items", self.heap_path()),
                Some(&self.admin.session),
                Some(json!({ "need": need, "title": "Target", "description": "Why and how" })),
            )
            .await?;
        let key = created.body["key"].as_str().context("item has no key")?.to_owned();
        let sent = self
            .app
            .send(
                Method::POST,
                &format!("{}/items/{key}/turn", self.heap_path()),
                Some(&self.admin.session),
                Some(json!({ "reach": "one" })),
            )
            .await?;
        anyhow::ensure!(sent.status == StatusCode::CREATED, "send failed: {}", sent.body);
        Ok(key)
    }

    /// Claims as `session` until the run holds `key`; the runs of other items it claims on the way stay live.
    /// Returns the run's ID.
    async fn claim_item(&self, session: &str, key: &str) -> Result<String> {
        let heaps = json!({ "heaps": [format!("{}/{}", self.team_slug, self.heap_slug)] });
        for _ in 0..50 {
            let claimed = self
                .app
                .send(Method::POST, "/api/v1/work/runs", Some(session), Some(heaps.clone()))
                .await?;
            anyhow::ensure!(claimed.status.is_success(), "claim failed: {}", claimed.body);
            let run = &claimed.body["run"];
            anyhow::ensure!(!run.is_null(), "nothing left to claim");
            if run["item"]["key"].as_str() == Some(key) {
                return Ok(run["id"].as_str().context("run has no id")?.to_owned());
            }
        }
        anyhow::bail!("{key} was not claimed")
    }

    /// Creates an item whose turn is stuck: three claims of it as the admin, each finished as failed. Returns its key.
    async fn create_stuck_item(&self) -> Result<String> {
        let key = self.create_item_with_turn("critical").await?;
        for _ in 0..3 {
            let run = self.claim_item(&self.admin.session, &key).await?;
            let finished = self
                .app
                .send(
                    Method::POST,
                    &format!("/api/v1/work/runs/{run}/finish"),
                    Some(&self.admin.session),
                    Some(json!({ "failure": "crashed" })),
                )
                .await?;
            anyhow::ensure!(finished.status == StatusCode::OK, "finish failed: {}", finished.body);
        }
        Ok(key)
    }

    /// Creates a tag as the admin and returns its ID and version.
    async fn create_tag(&self) -> Result<(String, String)> {
        let created = self
            .app
            .send(
                Method::POST,
                &format!("{}/tags", self.heap_path()),
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
                &format!("{}/views", self.heap_path()),
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
                &format!("{}/parameters", self.heap_path()),
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

    /// Adds a new user to the shared team as a member and returns their username.
    async fn new_member(&self) -> Result<String> {
        let (session, id) = self.new_user().await?;
        self.app.add_member(&self.team, &id, Role::Member).await?;
        let me = self.app.send(Method::GET, "/api/v1/me", Some(&session), None).await?;
        Ok(me.body["user"]["username"]
            .as_str()
            .context("user has no username")?
            .to_owned())
    }

    /// A new team whose admins are a new user and the shared team's admin, with the shared team's member in it.
    /// Returns its slug.
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
        Ok(created.body["slug"].as_str().context("team has no slug")?.to_owned())
    }

    /// The token of a working invite link of a new team, made by its new admin.
    async fn new_invite_token(&self) -> Result<String> {
        let (owner, _) = self.new_user().await?;
        let created = self
            .app
            .send(
                Method::POST,
                "/api/v1/teams",
                Some(&owner),
                Some(json!({ "name": "Foreign" })),
            )
            .await?;
        let slug = created.body["slug"].as_str().context("team has no slug")?;
        let made = self
            .app
            .send(
                Method::POST,
                &format!("/api/v1/teams/{slug}/invite-links"),
                Some(&owner),
                None,
            )
            .await?;
        Ok(made.body["token"].as_str().context("link has no token")?.to_owned())
    }

    /// The token of a working reset link of a new account.
    async fn new_reset_token(&self) -> Result<String> {
        let email = format!("{}@example.test", self.unique_name("user"));
        self.app.register(&email).await?;
        self.app.make_reset_link(&common::username_for(&email)).await
    }

    /// A new heap in the shared team, created by its admin. Returns its slug.
    async fn new_heap(&self) -> Result<String> {
        let created = self.create_heap().await?;
        Ok(created["slug"].as_str().context("heap has no slug")?.to_owned())
    }

    /// Makes a heap in the shared team as the admin and returns its body.
    async fn create_heap(&self) -> Result<Value> {
        let created = self
            .app
            .send(
                Method::POST,
                &format!("/api/v1/teams/{}/heaps", self.team_slug),
                Some(&self.admin.session),
                Some(json!({ "name": "Fresh" })),
            )
            .await?;
        anyhow::ensure!(
            created.status == StatusCode::CREATED,
            "create heap failed: {}",
            created.body
        );
        Ok(created.body)
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
                let username = self.unique_name("new");
                let email = format!("{username}@example.test");
                Some(json!({ "email": email, "username": username, "display_name": "New", "password": PASSWORD }))
            }
            RequestBody::SignIn => Some(json!({ "email": self.admin.email, "password": PASSWORD })),
            RequestBody::NewItem => Some(json!({ "title": "New", "need": "expected" })),
            RequestBody::Rename => Some(json!({ "title": "Renamed" })),
            RequestBody::MoveItem => Some(json!({ "heap_id": parameter["id"] })),
            RequestBody::NewStep => Some(json!({ "title": "A step" })),
            RequestBody::TickStep => Some(json!({ "state": "ticked" })),
            RequestBody::NewQuestion => Some(json!({ "kind": "question", "text": "Which CA?" })),
            RequestBody::ReplyToQuestion => Some(json!({ "reply": { "comment": "Internal" } })),
            RequestBody::SendToAgent => Some(json!({ "reach": "one" })),
            RequestBody::Claim => Some(json!({ "heaps": [format!("{}/{}", self.team_slug, self.heap_slug)] })),
            RequestBody::Reached => {
                Some(json!({ "result": "Added the context", "links": ["https://example.test/pr/1"] }))
            }
            RequestBody::Finish => Some(json!({})),
            RequestBody::NewTag => Some(json!({ "name": self.unique_name("new") })),
            RequestBody::RenameTag => Some(json!({ "name": self.unique_name("renamed") })),
            RequestBody::NewView => Some(json!({ "name": "Blocked", "view": { "blocked": "shown" } })),
            RequestBody::Name => Some(json!({ "name": self.unique_name("name") })),
            RequestBody::HeapContext => Some(json!({ "context": "Where the work is reviewed" })),
            RequestBody::Weights => Some(json!({ "need": 11 })),
            RequestBody::RankingPreview => Some(json!({})),
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
            RequestBody::NewPassword => Some(json!({ "password": PASSWORD })),
            RequestBody::ChangePassword => Some(json!({ "current_password": PASSWORD, "password": PASSWORD })),
            RequestBody::Authorization => Some(common::cli_authorization()),
            // Sent as forms by `call`.
            RequestBody::TokenExchange | RequestBody::Revocation => None,
        }
    }

    /// Prepares what `endpoint` needs, then calls it as `caller`, with the heap archived for the call when asked.
    async fn call(&self, endpoint: &Endpoint, caller: Caller, archived: bool) -> Result<TestResponse> {
        let account = self.account(caller);
        let mut session = account.map(|a| a.session.clone());
        let mut path = endpoint
            .path
            .replace("{heap}", &self.heap_slug)
            .replace("{team}", &self.team_slug);
        let mut if_match = None;
        let mut new_user = None;
        let mut parameter = Value::Null;
        let mut code = None;
        match endpoint.target {
            Target::Nothing | Target::Heap => {}
            Target::HeapVersion => {
                let heap = self
                    .app
                    .send(Method::GET, &self.heap_path(), Some(&self.admin.session), None)
                    .await?;
                if_match = Some(heap.body["version"].to_string());
            }
            Target::FreshHeap => {
                path = endpoint
                    .path
                    .replace("{heap}", &self.new_heap().await?)
                    .replace("{team}", &self.team_slug);
            }
            Target::Team => {
                let team = self
                    .app
                    .send(
                        Method::GET,
                        &format!("/api/v1/teams/{}", self.team_slug),
                        Some(&self.admin.session),
                        None,
                    )
                    .await?;
                if_match = Some(team.body["version"].to_string());
            }
            Target::ExtraMember => path = path.replace("{username}", &self.new_member().await?),
            Target::NewUser => new_user = Some(self.new_user().await?.1),
            Target::FreshTeam => path = endpoint.path.replace("{team}", &self.new_team().await?),
            Target::InviteLink => {
                let made = self
                    .app
                    .send(
                        Method::POST,
                        &format!("/api/v1/teams/{}/invite-links", self.team_slug),
                        Some(&self.admin.session),
                        None,
                    )
                    .await?;
                let id = made.body["id"].as_str().context("invite link has no id")?;
                path = path.replace("{id}", id);
            }
            Target::ForeignInviteLink => path = path.replace("{token}", &self.new_invite_token().await?),
            Target::ResetLink => path = path.replace("{token}", &self.new_reset_token().await?),
            Target::Me => {
                if let Some(session) = &session {
                    let me = self.app.send(Method::GET, "/api/v1/me", Some(session), None).await?;
                    if_match = Some(me.body["user"]["version"].to_string());
                }
            }
            Target::SignIn => {
                // Anonymous callers get a sign-in's shape and no sign-in, and are refused before it matters.
                let mut id = "7a1f9c2e-0d0b-4f5e-9a39-6c1e5d2b8f40".to_owned();
                if let Some(session) = &session {
                    self.app.sign_in_cli(session).await?;
                    let listed = self
                        .app
                        .send(Method::GET, "/api/v1/me/sign-ins", Some(session), None)
                        .await?;
                    id = listed.body["sign_ins"][0]["id"]
                        .as_str()
                        .context("no sign-in listed")?
                        .to_owned();
                }
                path = path.replace("{id}", &id);
            }
            Target::UserQuery => path.push_str("?query=example"),
            Target::AuthorizationQuery => path.push_str(&format!(
                "?response_type=code&client_id={}&redirect_uri={}&code_challenge={}&code_challenge_method=S256&state=s",
                common::CLI_CLIENT,
                "http%3A%2F%2F127.0.0.1%3A53682%2Fcallback",
                common::PKCE_CHALLENGE,
            )),
            Target::AuthorizationCode => code = Some(self.app.approve_cli(&self.admin.session).await?),
            Target::Queue => path.push_str(&format!("?heaps={}/{}", self.team_slug, self.heap_slug)),
            Target::Claimable => {
                self.create_item_with_turn("critical").await?;
            }
            Target::StuckItem => path = path.replace("{item}", &self.create_stuck_item().await?),
            Target::OwnRun => {
                let run = match session.as_deref() {
                    // Only a member of the team can claim in its heap.
                    Some(_) if matches!(caller, Caller::Outsider) => "7a1f9c2e-0d0b-4f5e-9a39-6c1e5d2b8f40".to_owned(),
                    Some(session) => {
                        let key = self.create_item_with_turn("critical").await?;
                        self.claim_item(session, &key).await?
                    }
                    None => "7a1f9c2e-0d0b-4f5e-9a39-6c1e5d2b8f40".to_owned(),
                };
                path = path.replace("{run}", &run);
            }
            Target::OpenItem | Target::ClosedItem => {
                let (key, version) = self.create_item(matches!(endpoint.target, Target::ClosedItem)).await?;
                path = path.replace("{item}", &key);
                if_match = Some(version);
            }
            Target::ItemAndSecondHeap => {
                let (key, version) = self.create_item(false).await?;
                path = path.replace("{item}", &key);
                if_match = Some(version);
                // The body reads the heap's ID from here, as it reads a parameter's.
                parameter = self.create_heap().await?;
            }
            Target::TaskWithStep => {
                let (key, version) = self.create_task_with_step().await?;
                path = path.replace("{item}", &key).replace("{step}", "1");
                if_match = Some(version);
            }
            Target::ItemWithQuestion => {
                let (key, version) = self.create_item_with_question().await?;
                path = path.replace("{item}", &key).replace("{question}", "1");
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
            Target::OwnAccount => {
                session = match account {
                    Some(_) => Some(self.new_user().await?.0),
                    None => None,
                };
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

        match endpoint.body {
            RequestBody::TokenExchange => return self.app.exchange_code(code.as_deref().unwrap_or_default()).await,
            RequestBody::Revocation => {
                return self
                    .app
                    .post_form("/api/v1/oauth/revoke", &[("token", "unknown")])
                    .await;
            }
            _ => {}
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
