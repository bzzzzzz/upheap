//! SCIM provisioning against a fake provider: the endpoints, what a push does to the group-managed teams, and what a
//! deactivation ends.

mod common;

use anyhow::{Context, Result};
use axum::http::{Method, StatusCode, header};
use common::oidc::{self, Claims, FakeProvider};
use common::{CSRF, TestApp, TestResponse};
use googletest::prelude::*;
use serde_json::{Value, json};

const TOKEN: &str = "scim-test-token-0123456789abcdef-0123456789";
const USER_SCHEMA: &str = "urn:ietf:params:scim:schemas:core:2.0:User";
const GROUP_SCHEMA: &str = "urn:ietf:params:scim:schemas:core:2.0:Group";
const PATCH_OP: &str = "urn:ietf:params:scim:api:messages:2.0:PatchOp";

struct Harness {
    provider: FakeProvider,
    /// The server with no provider and no group-managed team, where Ana sets Ops up.
    plain: TestApp,
    /// The same server with the provider and SCIM, and Ops managed by the groups `engineering` and `sre`.
    app: TestApp,
    ana: String,
}

async fn harness() -> Result<Harness> {
    let provider = FakeProvider::start().await?;
    let plain = TestApp::start().await?;
    let (ana, _) = plain.register_as("ana@example.test", "Ana").await?;
    let created_team = plain
        .send(
            Method::POST,
            "/api/v1/teams",
            Some(&ana),
            Some(json!({ "name": "Ops" })),
        )
        .await?;
    assert_eq!(created_team.status, StatusCode::CREATED, "{}", created_team.body);
    let app = plain
        .with_scim_groups(provider.settings()?, &[("ops", &["engineering", "sre"])], TOKEN)
        .await?;
    Ok(Harness {
        provider,
        plain,
        app,
        ana,
    })
}

/// A request to the SCIM endpoints with the operator's token.
async fn scim(h: &Harness, method: Method, path: &str, body: Option<Value>) -> Result<TestResponse> {
    scim_with(&h.app, method, path, Some(TOKEN), body).await
}

async fn scim_with(
    app: &TestApp,
    method: Method,
    path: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> Result<TestResponse> {
    let authorization = token.map(|token| format!("Bearer {token}"));
    let mut headers = Vec::new();
    if let Some(authorization) = &authorization {
        headers.push(("authorization", authorization.as_str()));
    }
    app.send_with(method, path, None, body, &headers).await
}

fn id_of(response: &TestResponse) -> Result<String> {
    Ok(response.body["id"].as_str().context("no id")?.to_owned())
}

/// Pushes a user and returns its SCIM id.
async fn push_user(h: &Harness, user_name: &str, external_id: Option<&str>, active: bool) -> Result<String> {
    let mut body = json!({ "schemas": [USER_SCHEMA], "userName": user_name, "active": active });
    if let Some(external_id) = external_id {
        body["externalId"] = json!(external_id);
    }
    let created = scim(h, Method::POST, "/scim/v2/Users", Some(body)).await?;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    id_of(&created)
}

/// Pushes a group with these members and returns its SCIM id.
async fn push_group(h: &Harness, display_name: &str, members: &[&str]) -> Result<String> {
    let members: Vec<Value> = members.iter().map(|id| json!({ "value": id })).collect();
    let created = scim(
        h,
        Method::POST,
        "/scim/v2/Groups",
        Some(json!({ "schemas": [GROUP_SCHEMA], "displayName": display_name, "members": members })),
    )
    .await?;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    id_of(&created)
}

async fn patch(h: &Harness, path: &str, operations: Value) -> Result<TestResponse> {
    scim(
        h,
        Method::PATCH,
        path,
        Some(json!({ "schemas": [PATCH_OP], "Operations": operations })),
    )
    .await
}

fn sam() -> Claims {
    Claims::verified("subject-sam", "sam@example.test", "Sam Doe").with_claim("groups", json!([]))
}

/// Starts a sign-in at the provider and returns the callback's answer after it vouched for `claims`, and the state.
async fn sign_in(h: &Harness, claims: Claims) -> Result<(TestResponse, String)> {
    let start = h
        .app
        .send_with(Method::GET, "/api/v1/auth/oidc/start", None, None, &[])
        .await?;
    let location = start.headers.get(header::LOCATION).context("no Location")?.to_str()?;
    let request = oidc::parse_authorization(location)?;
    let code = h.provider.code_for(&request, claims);
    let state = request.state;
    let answered = h
        .app
        .send_with(
            Method::GET,
            &format!("/api/v1/auth/oidc/callback?code={code}&state={state}"),
            None,
            None,
            &[("cookie", &format!("upheap_oidc={state}"))],
        )
        .await?;
    Ok((answered, state))
}

/// Sam's first sign-in through the username step. Returns their session.
async fn first_sign_in(h: &Harness, claims: Claims) -> Result<String> {
    let (_, state) = sign_in(h, claims).await?;
    let registered = h
        .app
        .send_with(
            Method::POST,
            "/api/v1/auth/oidc/register",
            None,
            Some(json!({ "username": "sam" })),
            &[CSRF, ("cookie", &format!("upheap_oidc={state}"))],
        )
        .await?;
    assert_eq!(registered.status, StatusCode::CREATED, "{}", registered.body);
    registered.session_cookie.context("no session cookie")
}

async fn me(app: &TestApp, session: &str) -> Result<TestResponse> {
    app.send(Method::GET, "/api/v1/me", Some(session), None).await
}

fn in_ops(me: &TestResponse) -> bool {
    me.body["teams"]
        .as_array()
        .is_some_and(|teams| teams.iter().any(|team| team["slug"] == "ops"))
}

fn user_id_of(me: &TestResponse) -> Result<String> {
    Ok(me.body["user"]["id"].as_str().context("no user id")?.to_owned())
}

#[gtest]
#[tokio::test]
async fn a_request_without_the_operators_token_is_refused_in_scim_format() -> Result<()> {
    let h = harness().await?;

    let none = scim_with(&h.app, Method::GET, "/scim/v2/Users", None, None).await?;
    let wrong = scim_with(&h.app, Method::GET, "/scim/v2/Users", Some("not-the-token"), None).await?;
    let config = scim_with(
        &h.app,
        Method::GET,
        "/scim/v2/ServiceProviderConfig",
        Some("nope"),
        None,
    )
    .await?;

    for refused in [&none, &wrong, &config] {
        expect_that!(refused.status, eq(StatusCode::UNAUTHORIZED));
        expect_that!(
            refused.body,
            eq(&json!({
                "schemas": ["urn:ietf:params:scim:api:messages:2.0:Error"],
                "status": "401",
                "detail": "the bearer token is missing or wrong",
            }))
        );
        expect_that!(
            refused
                .headers
                .get(header::WWW_AUTHENTICATE)
                .map(|v| v.to_str().unwrap_or_default()),
            some(eq("Bearer"))
        );
        expect_that!(
            refused
                .headers
                .get(header::CONTENT_TYPE)
                .map(|v| v.to_str().unwrap_or_default()),
            some(eq("application/scim+json"))
        );
    }
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_routes_are_absent_without_a_token_and_the_token_is_no_api_credential() -> Result<()> {
    let h = harness().await?;

    let absent = scim_with(&h.plain, Method::GET, "/scim/v2/Users", Some(TOKEN), None).await?;
    let api = h.app.send_bearer(Method::GET, "/api/v1/me", TOKEN, None).await?;

    expect_that!(absent.status, eq(StatusCode::NOT_FOUND));
    expect_that!(absent.body, eq(&Value::Null));
    expect_that!(api.status, eq(StatusCode::UNAUTHORIZED));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_service_provider_config_says_what_is_supported() -> Result<()> {
    let h = harness().await?;

    let config = scim(&h, Method::GET, "/scim/v2/ServiceProviderConfig", None).await?;

    expect_that!(config.status, eq(StatusCode::OK));
    expect_that!(config.body["patch"]["supported"], eq(&json!(true)));
    expect_that!(config.body["bulk"]["supported"], eq(&json!(false)));
    expect_that!(config.body["filter"]["maxResults"], eq(&json!(100)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_user_is_created_read_replaced_patched_and_deleted() -> Result<()> {
    // Arrange
    let h = harness().await?;
    let created = scim(
        &h,
        Method::POST,
        "/scim/v2/Users",
        Some(json!({
            "schemas": [USER_SCHEMA],
            "userName": "sam@example.test",
            "externalId": "subject-sam",
            "name": { "givenName": "Sam" },
            "password": "never stored",
            "id": "ignored",
            "meta": { "resourceType": "Nope" },
        })),
    )
    .await?;
    let id = id_of(&created)?;
    let url = format!("/scim/v2/Users/{id}");

    // Act
    let read = scim(&h, Method::GET, &url, None).await?;
    let replaced = scim(
        &h,
        Method::PUT,
        &url,
        Some(json!({ "userName": "sam@example.test", "externalId": "subject-sam", "title": "Dev", "active": true })),
    )
    .await?;
    let patched = patch(&h, &url, json!([{ "op": "replace", "value": { "active": false } }])).await?;
    let deleted = scim(&h, Method::DELETE, &url, None).await?;
    let gone = scim(&h, Method::GET, &url, None).await?;

    // Assert
    expect_that!(created.status, eq(StatusCode::CREATED));
    expect_that!(
        created
            .headers
            .get(header::LOCATION)
            .map(|v| v.to_str().unwrap_or_default()),
        some(eq(format!("{}{url}", oidc::APP_URL).as_str()))
    );
    expect_that!(created.body["schemas"], eq(&json!([USER_SCHEMA])));
    expect_that!(created.body["active"], eq(&json!(true)));
    expect_that!(created.body["name"], eq(&json!({ "givenName": "Sam" })));
    expect_that!(created.body.get("password"), none());
    expect_that!(created.body["meta"]["resourceType"], eq(&json!("User")));
    expect_that!(
        created.body["meta"]["location"],
        eq(&json!(format!("{}{url}", oidc::APP_URL)))
    );
    expect_that!(read.body, eq(&created.body));
    expect_that!(replaced.status, eq(StatusCode::OK));
    expect_that!(replaced.body["title"], eq(&json!("Dev")));
    expect_that!(replaced.body.get("name"), none());
    expect_that!(patched.status, eq(StatusCode::OK));
    expect_that!(patched.body["active"], eq(&json!(false)));
    expect_that!(patched.body["title"], eq(&json!("Dev")));
    expect_that!(deleted.status, eq(StatusCode::NO_CONTENT));
    expect_that!(gone.status, eq(StatusCode::NOT_FOUND));
    expect_that!(gone.body["status"], eq(&json!("404")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_taken_user_name_is_a_uniqueness_conflict_and_a_bad_body_is_refused() -> Result<()> {
    let h = harness().await?;
    push_user(&h, "sam@example.test", Some("subject-sam"), true).await?;

    let taken = scim(
        &h,
        Method::POST,
        "/scim/v2/Users",
        Some(json!({ "userName": "sam@example.test" })),
    )
    .await?;
    let same_external = scim(
        &h,
        Method::POST,
        "/scim/v2/Users",
        Some(json!({ "userName": "other@example.test", "externalId": "subject-sam" })),
    )
    .await?;
    let no_name = scim(&h, Method::POST, "/scim/v2/Users", Some(json!({ "externalId": "x" }))).await?;
    let not_json = h
        .app
        .send_with(
            Method::POST,
            "/scim/v2/Users",
            None,
            None,
            &[
                ("authorization", &format!("Bearer {TOKEN}")),
                ("content-type", "application/scim+json"),
            ],
        )
        .await?;
    let bad_id = scim(&h, Method::GET, "/scim/v2/Users/not-an-id", None).await?;

    expect_that!(taken.status, eq(StatusCode::CONFLICT));
    expect_that!(taken.body["scimType"], eq(&json!("uniqueness")));
    expect_that!(taken.body["status"], eq(&json!("409")));
    expect_that!(same_external.status, eq(StatusCode::CREATED));
    expect_that!(no_name.status, eq(StatusCode::BAD_REQUEST));
    expect_that!(no_name.body["scimType"], eq(&json!("invalidValue")));
    expect_that!(not_json.status, eq(StatusCode::BAD_REQUEST));
    expect_that!(not_json.body["scimType"], eq(&json!("invalidSyntax")));
    expect_that!(bad_id.status, eq(StatusCode::NOT_FOUND));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn users_are_listed_in_pages_and_filtered() -> Result<()> {
    // Arrange
    let h = harness().await?;
    for n in 1..=3 {
        h.app.clock.advance(domain::Duration::from_millis(1000));
        push_user(
            &h,
            &format!("user{n}@example.test"),
            Some(&format!("subject-{n}")),
            true,
        )
        .await?;
    }

    // Act
    let all = scim(&h, Method::GET, "/scim/v2/Users", None).await?;
    let second = scim(&h, Method::GET, "/scim/v2/Users?startIndex=2&count=1", None).await?;
    let clamped = scim(&h, Method::GET, "/scim/v2/Users?startIndex=-4&count=-1", None).await?;
    let by_name = scim(
        &h,
        Method::GET,
        "/scim/v2/Users?filter=USERNAME%20EQ%20%22user3%40example.test%22",
        None,
    )
    .await?;
    let by_external = scim(
        &h,
        Method::GET,
        "/scim/v2/Users?filter=externalId%20eq%20%22subject-2%22",
        None,
    )
    .await?;
    let nobody = scim(
        &h,
        Method::GET,
        "/scim/v2/Users?filter=userName%20eq%20%22nobody%22",
        None,
    )
    .await?;
    let bad = scim(
        &h,
        Method::GET,
        "/scim/v2/Users?filter=userName%20co%20%22user%22",
        None,
    )
    .await?;
    let names = |response: &TestResponse| -> Vec<String> {
        response.body["Resources"]
            .as_array()
            .map(|all| {
                all.iter()
                    .filter_map(|r| r["userName"].as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    };

    // Assert
    expect_that!(
        all.body["schemas"],
        eq(&json!(["urn:ietf:params:scim:api:messages:2.0:ListResponse"]))
    );
    expect_that!(all.body["totalResults"], eq(&json!(3)));
    expect_that!(all.body["startIndex"], eq(&json!(1)));
    expect_that!(
        names(&all),
        elements_are![
            eq("user1@example.test"),
            eq("user2@example.test"),
            eq("user3@example.test")
        ]
    );
    expect_that!(names(&second), elements_are![eq("user2@example.test")]);
    expect_that!(second.body["totalResults"], eq(&json!(3)));
    expect_that!(second.body["itemsPerPage"], eq(&json!(1)));
    expect_that!(second.body["startIndex"], eq(&json!(2)));
    expect_that!(names(&clamped), is_empty());
    expect_that!(clamped.body["startIndex"], eq(&json!(1)));
    expect_that!(clamped.body["totalResults"], eq(&json!(3)));
    expect_that!(names(&by_name), elements_are![eq("user3@example.test")]);
    expect_that!(names(&by_external), elements_are![eq("user2@example.test")]);
    expect_that!(nobody.body["totalResults"], eq(&json!(0)));
    expect_that!(bad.status, eq(StatusCode::BAD_REQUEST));
    expect_that!(bad.body["scimType"], eq(&json!("invalidFilter")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_group_keeps_its_members_and_refuses_one_that_is_no_user() -> Result<()> {
    // Arrange
    let h = harness().await?;
    let ana = push_user(&h, "ana@example.test", None, true).await?;
    let blair = push_user(&h, "blair@example.test", None, true).await?;
    let group = push_group(&h, "eng", &[&ana]).await?;
    let url = format!("/scim/v2/Groups/{group}");

    // Act
    let unknown = scim(
        &h,
        Method::POST,
        "/scim/v2/Groups",
        Some(json!({ "displayName": "other", "members": [{ "value": "00000000-0000-0000-0000-000000000000" }] })),
    )
    .await?;
    let taken = scim(
        &h,
        Method::POST,
        "/scim/v2/Groups",
        Some(json!({ "displayName": "eng" })),
    )
    .await?;
    let added = patch(
        &h,
        &url,
        json!([{ "op": "add", "path": "members", "value": [{ "value": blair }] }]),
    )
    .await?;
    let removed = patch(
        &h,
        &url,
        json!([{ "op": "remove", "path": format!("members[value eq \"{ana}\"]") }]),
    )
    .await?;
    let renamed = patch(
        &h,
        &url,
        json!([{ "op": "replace", "path": "displayName", "value": "engineers" }]),
    )
    .await?;
    let replaced = scim(
        &h,
        Method::PUT,
        &url,
        Some(json!({ "displayName": "engineers", "externalId": "g-1" })),
    )
    .await?;
    let by_name = scim(
        &h,
        Method::GET,
        "/scim/v2/Groups?filter=displayName%20eq%20%22engineers%22",
        None,
    )
    .await?;
    let by_external = scim(
        &h,
        Method::GET,
        "/scim/v2/Groups?filter=externalId%20eq%20%22g-1%22",
        None,
    )
    .await?;
    let deleted = scim(&h, Method::DELETE, &url, None).await?;
    let gone = scim(&h, Method::GET, &url, None).await?;

    // Assert
    expect_that!(unknown.status, eq(StatusCode::BAD_REQUEST));
    expect_that!(unknown.body["scimType"], eq(&json!("invalidValue")));
    expect_that!(taken.status, eq(StatusCode::CONFLICT));
    expect_that!(taken.body["scimType"], eq(&json!("uniqueness")));
    expect_that!(
        added.body["members"],
        eq(&json!([{ "value": ana }, { "value": blair }]))
    );
    expect_that!(removed.body["members"], eq(&json!([{ "value": blair }])));
    expect_that!(renamed.body["displayName"], eq(&json!("engineers")));
    expect_that!(renamed.body["schemas"], eq(&json!([GROUP_SCHEMA])));
    expect_that!(replaced.body["members"], eq(&json!([])));
    expect_that!(by_name.body["totalResults"], eq(&json!(1)));
    expect_that!(by_external.body["Resources"][0]["id"], eq(&json!(group)));
    expect_that!(deleted.status, eq(StatusCode::NO_CONTENT));
    expect_that!(gone.status, eq(StatusCode::NOT_FOUND));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_group_push_adds_and_removes_a_linked_member_with_audit_entries() -> Result<()> {
    // Arrange: Sam signed in with no groups, so is in no team.
    let h = harness().await?;
    let session = first_sign_in(&h, sam()).await?;
    let user_id = user_id_of(&me(&h.app, &session).await?)?;
    let scim_sam = push_user(&h, "sam@example.test", Some("subject-sam"), true).await?;
    let group = push_group(&h, "unrelated", &[&scim_sam]).await?;
    let url = format!("/scim/v2/Groups/{group}");
    let before = in_ops(&me(&h.app, &session).await?);

    // Act: the group is renamed to a mapped name, then Sam is taken out of it.
    patch(
        &h,
        &url,
        json!([{ "op": "replace", "path": "displayName", "value": "engineering" }]),
    )
    .await?;
    let joined = in_ops(&me(&h.app, &session).await?);
    patch(
        &h,
        &url,
        json!([{ "op": "remove", "path": format!("members[value eq \"{scim_sam}\"]") }]),
    )
    .await?;
    let left = in_ops(&me(&h.app, &session).await?);

    // Assert
    expect_that!(before, eq(false));
    expect_that!(joined, eq(true));
    expect_that!(left, eq(false));
    let entries = h.app.audit_entries(&user_id).await?;
    let actions: Vec<_> = entries
        .iter()
        .filter(|(action, _)| action.starts_with("group_member_"))
        .map(|(action, diff)| (action.as_str(), diff.clone()))
        .collect();
    expect_that!(
        actions,
        elements_are![
            eq(&(
                "group_member_added",
                json!({ "groups": ["engineering"], "by": "provider_groups" })
            )),
            eq(&(
                "group_member_removed",
                json!({ "role": "member", "groups": ["engineering", "sre"], "by": "provider_groups" })
            )),
        ]
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_group_matches_by_its_external_id_and_unmapped_groups_and_unlinked_people_change_nothing() -> Result<()> {
    // Arrange
    let h = harness().await?;
    let session = first_sign_in(&h, sam()).await?;
    let scim_sam = push_user(&h, "sam@example.test", Some("subject-sam"), true).await?;
    let stranger = push_user(&h, "stranger@example.test", Some("subject-stranger"), true).await?;
    let no_external = push_user(&h, "nobody@example.test", None, true).await?;
    let team_members = h.app.count_rows("team_members").await?;

    // Act
    push_group(&h, "unrelated", &[&scim_sam]).await?;
    push_group(&h, "engineering", &[&stranger, &no_external]).await?;
    let in_unmapped = in_ops(&me(&h.app, &session).await?);
    let by_external = scim(
        &h,
        Method::POST,
        "/scim/v2/Groups",
        Some(json!({ "displayName": "Site reliability", "externalId": "sre", "members": [{ "value": scim_sam }] })),
    )
    .await?;
    let in_external = in_ops(&me(&h.app, &session).await?);

    // Assert
    expect_that!(in_unmapped, eq(false));
    expect_that!(h.app.count_rows("team_members").await?, eq(team_members + 1));
    expect_that!(by_external.status, eq(StatusCode::CREATED));
    expect_that!(in_external, eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn deactivating_a_linked_person_ends_their_teams_and_sign_ins_and_refuses_new_ones() -> Result<()> {
    // Arrange: Sam is in Ops through a pushed group, with a session, a CLI sign-in and a personal token.
    let h = harness().await?;
    let session = first_sign_in(&h, sam()).await?;
    let scim_sam = push_user(&h, "sam@example.test", Some("subject-sam"), true).await?;
    push_group(&h, "engineering", &[&scim_sam]).await?;
    let secret = h.app.make_personal_token(&session, "ci", None).await?.body["secret"]
        .as_str()
        .context("no secret")?
        .to_owned();
    let (access, _) = h.app.sign_in_cli(&session).await?;
    let user_id = user_id_of(&me(&h.app, &session).await?)?;
    assert!(in_ops(&me(&h.app, &session).await?));

    // Act
    let deactivated = patch(
        &h,
        &format!("/scim/v2/Users/{scim_sam}"),
        json!([{ "op": "replace", "value": { "active": false } }]),
    )
    .await?;
    let again = patch(
        &h,
        &format!("/scim/v2/Users/{scim_sam}"),
        json!([{ "op": "replace", "path": "active", "value": false }]),
    )
    .await?;

    // Assert: out of the team, every sign-in ended.
    expect_that!(deactivated.status, eq(StatusCode::OK));
    expect_that!(again.status, eq(StatusCode::OK));
    expect_that!(me(&h.app, &session).await?.status, eq(StatusCode::UNAUTHORIZED));
    expect_that!(
        h.app
            .send_bearer(Method::GET, "/api/v1/me", &secret, None)
            .await?
            .status,
        eq(StatusCode::UNAUTHORIZED)
    );
    expect_that!(
        h.app
            .send_bearer(Method::GET, "/api/v1/me", &access, None)
            .await?
            .status,
        eq(StatusCode::UNAUTHORIZED)
    );
    let removed: Vec<_> = h
        .app
        .audit_entries(&user_id)
        .await?
        .into_iter()
        .filter(|(action, _)| action == "group_member_removed")
        .collect();
    expect_that!(removed, len(eq(1)));

    // A provider sign-in is refused, and so is a password reset, which then changes nothing.
    let (refused, _) = sign_in(&h, sam()).await?;
    expect_that!(refused.status, eq(StatusCode::FORBIDDEN));
    expect_that!(refused.body["code"], eq(&json!("account_deactivated")));
    let link = h.app.make_reset_link("sam").await?;
    let reset = h
        .app
        .send(
            Method::POST,
            &format!("/api/v1/password-resets/{link}"),
            None,
            Some(json!({ "password": "another long password" })),
        )
        .await?;
    expect_that!(reset.status, eq(StatusCode::FORBIDDEN));
    expect_that!(reset.body["code"], eq(&json!("account_deactivated")));
    expect_that!(reset.session_cookie, none());
    let password_sign_in = h
        .app
        .send(
            Method::POST,
            "/api/v1/auth/login",
            None,
            Some(json!({ "login": "sam", "password": "another long password" })),
        )
        .await?;
    expect_that!(password_sign_in.status, eq(StatusCode::UNAUTHORIZED));

    // Reactivated, the provider sign-in works and the groups come back.
    patch(
        &h,
        &format!("/scim/v2/Users/{scim_sam}"),
        json!([{ "op": "replace", "value": { "active": true } }]),
    )
    .await?;
    let (back, _) = sign_in(&h, sam()).await?;
    expect_that!(back.status, eq(StatusCode::SEE_OTHER));
    let session = back.session_cookie.context("no session")?;
    expect_that!(in_ops(&me(&h.app, &session).await?), eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn deleting_a_linked_person_takes_them_out_of_their_teams_and_ends_their_sign_ins() -> Result<()> {
    // Arrange
    let h = harness().await?;
    let session = first_sign_in(&h, sam()).await?;
    let scim_sam = push_user(&h, "sam@example.test", Some("subject-sam"), true).await?;
    push_group(&h, "sre", &[&scim_sam]).await?;
    assert!(in_ops(&me(&h.app, &session).await?));
    let other = h.plain.register_as("blair@example.test", "Blair").await?.0;

    // Act
    let deleted = scim(&h, Method::DELETE, &format!("/scim/v2/Users/{scim_sam}"), None).await?;

    // Assert
    expect_that!(deleted.status, eq(StatusCode::NO_CONTENT));
    expect_that!(me(&h.app, &session).await?.status, eq(StatusCode::UNAUTHORIZED));
    expect_that!(me(&h.app, &other).await?.status, eq(StatusCode::OK));
    let (again, _) = sign_in(&h, sam()).await?;
    expect_that!(again.status, eq(StatusCode::SEE_OTHER));
    let session = again.session_cookie.context("no session")?;
    expect_that!(in_ops(&me(&h.app, &session).await?), eq(false));
    expect_that!(h.app.count_rows("scim_group_members").await?, eq(0));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_sign_in_takes_its_groups_from_the_pushed_record_and_a_person_pushed_first_gets_their_teams() -> Result<()> {
    // Arrange: Sam is pushed into `engineering` before they ever sign in.
    let h = harness().await?;
    let scim_sam = push_user(&h, "sam@example.test", Some("subject-sam"), true).await?;
    let group = push_group(&h, "engineering", &[&scim_sam]).await?;

    // Act: the token says no groups at all.
    let session = first_sign_in(&h, sam()).await?;
    let joined = in_ops(&me(&h.app, &session).await?);

    // The provider then takes Sam out of the group, but a token minted before still lists it.
    scim(
        &h,
        Method::PATCH,
        &format!("/scim/v2/Groups/{group}"),
        Some(json!({ "Operations": [{ "op": "remove", "path": "members" }] })),
    )
    .await?;
    let stale =
        Claims::verified("subject-sam", "sam@example.test", "Sam Doe").with_claim("groups", json!(["engineering"]));
    let (answered, _) = sign_in(&h, stale).await?;
    let session = answered.session_cookie.context("no session")?;

    // Assert
    expect_that!(joined, eq(true));
    expect_that!(in_ops(&me(&h.app, &session).await?), eq(false));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn deactivating_a_person_with_no_linked_account_changes_nothing_in_upheap() -> Result<()> {
    // Arrange: Ana has a local account, no provider identity, and the same address the provider has for her.
    let h = harness().await?;
    let scim_ana = push_user(&h, "ana@example.test", Some("subject-ana"), true).await?;

    // Act
    let deactivated = patch(
        &h,
        &format!("/scim/v2/Users/{scim_ana}"),
        json!([{ "op": "replace", "value": { "active": false } }]),
    )
    .await?;

    // Assert
    expect_that!(deactivated.status, eq(StatusCode::OK));
    let ana = me(&h.app, &h.ana).await?;
    expect_that!(ana.status, eq(StatusCode::OK));
    expect_that!(in_ops(&ana), eq(true));
    Ok(())
}
