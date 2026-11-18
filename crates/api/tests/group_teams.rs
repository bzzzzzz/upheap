//! Teams whose members come from the provider's groups, against a fake provider: joining and leaving at sign-in, a
//! token that doesn't say, and what a group-managed team refuses.

mod common;

use anyhow::{Context, Result};
use axum::http::{Method, StatusCode, header};
use common::oidc::{self, Claims, FakeProvider};
use common::{CSRF, TestApp, TestResponse};
use googletest::prelude::*;
use serde_json::{Value, json};

struct Harness {
    provider: FakeProvider,
    /// The server with no provider and no group-managed team, where Ana sets Ops up.
    plain: TestApp,
    /// The same server with the provider, and Ops managed by the groups `engineering` and `sre`.
    app: TestApp,
    ana: String,
    team_id: String,
    team_url: String,
}

/// Ana, admin of the shared team Ops, which the provider's groups `engineering` and `sre` manage.
async fn harness() -> Result<Harness> {
    let provider = FakeProvider::start().await?;
    let plain = TestApp::start().await?;
    let (ana, _) = plain.register_as("ana@example.test", "Ana").await?;
    let created = plain
        .send(
            Method::POST,
            "/api/v1/teams",
            Some(&ana),
            Some(json!({ "name": "Ops" })),
        )
        .await?;
    let team_id = created.body["id"].as_str().context("team has no id")?.to_owned();
    let team_url = plain.team_url(&team_id).await?;
    let app = plain
        .with_oidc_groups(provider.settings()?, &[("ops", &["engineering", "sre"])])
        .await?;
    Ok(Harness {
        provider,
        plain,
        app,
        ana,
        team_id,
        team_url,
    })
}

fn sam(groups: Value) -> Claims {
    Claims::verified("subject-sam", "sam@example.test", "Sam Doe").with_claim("groups", groups)
}

/// Starts a sign-in at the provider and returns the callback's answer after it vouched for `claims`.
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

/// Sam's first sign-in with `groups`, through the username step. Returns their session.
async fn first_sign_in(h: &Harness, groups: Value) -> Result<String> {
    let (_, state) = sign_in(h, sam(groups)).await?;
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

/// Sam signing in again with `groups`. Returns their session.
async fn sign_in_again(h: &Harness, groups: Value) -> Result<String> {
    let (answered, _) = sign_in(h, sam(groups)).await?;
    assert_eq!(answered.status, StatusCode::SEE_OTHER, "{}", answered.body);
    answered.session_cookie.context("no session cookie")
}

async fn me(app: &TestApp, session: &str) -> Result<Value> {
    let me = app.send(Method::GET, "/api/v1/me", Some(session), None).await?;
    Ok(me.body)
}

/// The team Ops among `me`'s teams.
fn ops_of(me: &Value) -> Option<&Value> {
    me["teams"].as_array()?.iter().find(|team| team["slug"] == "ops")
}

fn user_id_of(me: &Value) -> Result<String> {
    Ok(me["user"]["id"].as_str().context("no user id")?.to_owned())
}

async fn make_admin(h: &Harness, user_id: &str) -> Result<()> {
    let url = h.app.member_url(&h.team_id, user_id).await?;
    let changed = h
        .app
        .send(Method::PATCH, &url, Some(&h.ana), Some(json!({ "role": "admin" })))
        .await?;
    assert_eq!(changed.status, StatusCode::OK, "{}", changed.body);
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_first_sign_in_with_a_mapped_group_joins_the_team_after_the_username_step() -> Result<()> {
    // Arrange
    let h = harness().await?;

    // Act
    let session = first_sign_in(&h, json!(["sre", "unrelated"])).await?;

    // Assert
    let me = me(&h.app, &session).await?;
    let ops = ops_of(&me).context("Sam is not in Ops")?;
    expect_that!(ops["role"], eq(&json!("member")));
    let entries = h.app.audit_entries(&user_id_of(&me)?).await?;
    let added: Vec<_> = entries
        .iter()
        .filter(|(action, _)| action == "group_member_added")
        .collect();
    expect_that!(added, len(eq(1)));
    expect_that!(added[0].1, eq(&json!({ "groups": ["sre"], "by": "provider_groups" })));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_first_sign_in_without_a_mapped_group_joins_nothing() -> Result<()> {
    let h = harness().await?;

    let session = first_sign_in(&h, json!(["unrelated"])).await?;

    expect_that!(ops_of(&me(&h.app, &session).await?), none());
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_member_whose_groups_are_gone_is_removed_even_as_the_last_admin() -> Result<()> {
    // Arrange: Sam is the only admin of Ops once Ana has left.
    let h = harness().await?;
    let session = first_sign_in(&h, json!(["engineering"])).await?;
    let user_id = user_id_of(&me(&h.app, &session).await?)?;
    make_admin(&h, &user_id).await?;
    let left = h
        .app
        .send(Method::POST, &format!("{}/leave", h.team_url), Some(&h.ana), None)
        .await?;
    assert_eq!(left.status, StatusCode::NO_CONTENT, "{}", left.body);

    // Act
    let session = sign_in_again(&h, json!(["unrelated"])).await?;

    // Assert
    expect_that!(ops_of(&me(&h.app, &session).await?), none());
    let entries = h.app.audit_entries(&user_id).await?;
    let removed = entries
        .iter()
        .find(|(action, _)| action == "group_member_removed")
        .context("no removal recorded")?;
    expect_that!(
        removed.1,
        eq(&json!({ "role": "admin", "groups": ["engineering", "sre"], "by": "provider_groups" }))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_empty_list_removes_and_a_missing_claim_keeps() -> Result<()> {
    // Arrange
    let h = harness().await?;
    first_sign_in(&h, json!(["engineering"])).await?;

    // Act: a token without the claim, then one with an empty list.
    let (unknown, _) = sign_in(&h, Claims::verified("subject-sam", "sam@example.test", "Sam Doe")).await?;
    let kept = me(&h.app, &unknown.session_cookie.context("no session")?).await?;
    let session = sign_in_again(&h, json!([])).await?;
    let emptied = me(&h.app, &session).await?;

    // Assert
    expect_that!(ops_of(&kept), some(anything()));
    expect_that!(ops_of(&emptied), none());
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_claim_that_is_one_string_is_one_group() -> Result<()> {
    let h = harness().await?;

    let session = first_sign_in(&h, json!("engineering")).await?;

    expect_that!(ops_of(&me(&h.app, &session).await?), some(anything()));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_group_managed_team_refuses_hand_adds_invite_links_and_a_new_slug_but_renames() -> Result<()> {
    // Arrange
    let h = harness().await?;
    let session = first_sign_in(&h, json!(["engineering"])).await?;
    let user_id = user_id_of(&me(&h.app, &session).await?)?;
    let team = h.app.send(Method::GET, &h.team_url, Some(&h.ana), None).await?;

    // Act
    let added = h
        .app
        .send(
            Method::POST,
            &format!("{}/members", h.team_url),
            Some(&h.ana),
            Some(json!({ "user_id": user_id })),
        )
        .await?;
    let link = h
        .app
        .send(
            Method::POST,
            &format!("{}/invite-links", h.team_url),
            Some(&h.ana),
            None,
        )
        .await?;
    let slug = h
        .app
        .patch(
            &h.ana,
            &h.team_url,
            &team.body["version"],
            json!({ "slug": "operations" }),
        )
        .await?;
    let renamed = h
        .app
        .patch(
            &h.ana,
            &h.team_url,
            &team.body["version"],
            json!({ "name": "Operations" }),
        )
        .await?;

    // Assert
    for refused in [&added, &link, &slug] {
        expect_that!(refused.status, eq(StatusCode::CONFLICT), "{}", refused.body);
        expect_that!(refused.body["code"], eq(&json!("team_group_managed")));
    }
    expect_that!(renamed.status, eq(StatusCode::OK));
    expect_that!(renamed.body["name"], eq(&json!("Operations")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_link_made_before_the_mapping_no_longer_admits_anyone() -> Result<()> {
    // Arrange: the link comes from the server as it was before Ops was mapped.
    let h = harness().await?;
    let link = h
        .plain
        .send(
            Method::POST,
            &format!("{}/invite-links", h.team_url),
            Some(&h.ana),
            None,
        )
        .await?;
    let token = link.body["token"].as_str().context("no token")?.to_owned();
    let (blair, _) = h.plain.register_as("blair@example.test", "Blair").await?;
    let users = h.app.count_rows("users").await?;

    // Act
    let opened = h
        .app
        .send(Method::GET, &format!("/api/v1/invites/{token}"), Some(&blair), None)
        .await?;
    let signed_out = h
        .app
        .send(Method::GET, &format!("/api/v1/invites/{token}"), None, None)
        .await?;
    let joined = h
        .app
        .send(
            Method::POST,
            &format!("/api/v1/invites/{token}/join"),
            Some(&blair),
            None,
        )
        .await?;
    let registered = h
        .app
        .send(
            Method::POST,
            &format!("/api/v1/invites/{token}/register"),
            None,
            Some(json!({
                "email": "newcomer@example.test",
                "username": "newcomer",
                "display_name": "Newcomer",
                "password": "a long test password",
            })),
        )
        .await?;

    // Assert
    expect_that!(opened.body["group_managed"], eq(&json!(true)));
    expect_that!(opened.body["state"], eq(&json!("working")));
    expect_that!(opened.body["allowed_actions"], eq(&json!([])));
    expect_that!(signed_out.body["allowed_actions"], eq(&json!([])));
    for refused in [&joined, &registered] {
        expect_that!(refused.status, eq(StatusCode::CONFLICT), "{}", refused.body);
        expect_that!(refused.body["code"], eq(&json!("team_group_managed")));
    }
    expect_that!(h.app.count_rows("users").await?, eq(users));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_team_and_me_say_which_groups_manage_it() -> Result<()> {
    // Arrange
    let h = harness().await?;

    // Act
    let team = h.app.send(Method::GET, &h.team_url, Some(&h.ana), None).await?;
    let me = me(&h.app, &h.ana).await?;
    let unmanaged = h.plain.send(Method::GET, &h.team_url, Some(&h.ana), None).await?;
    let unmanaged_me = self::me(&h.plain, &h.ana).await?;

    // Assert
    expect_that!(team.body["managed_by_groups"], eq(&json!(["engineering", "sre"])));
    let actions = team.body["allowed_actions"].as_array().context("no actions")?;
    for hidden in ["change_slug", "add_member", "manage_invite_links"] {
        expect_that!(actions, not(contains(eq(&json!(hidden)))));
    }
    expect_that!(ops_of(&me).context("no Ops")?["group_managed"], eq(&json!(true)));
    expect_that!(me["teams"][0]["group_managed"], eq(&json!(false)));
    expect_that!(unmanaged.body["managed_by_groups"], eq(&Value::Null));
    expect_that!(
        ops_of(&unmanaged_me).context("no Ops")?["group_managed"],
        eq(&json!(false))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_team_activity_says_the_groups_in_words() -> Result<()> {
    // Arrange
    let h = harness().await?;
    first_sign_in(&h, json!(["engineering"])).await?;
    sign_in_again(&h, json!([])).await?;

    // Act
    let audit = h
        .app
        .send(Method::GET, &format!("{}/audit", h.team_url), Some(&h.ana), None)
        .await?;

    // Assert
    let entries = audit.body["entries"].as_array().context("no entries")?;
    let phrases: Vec<&str> = entries.iter().filter_map(|e| e["phrase"].as_str()).collect();
    expect_that!(
        phrases,
        superset_of([
            &"left: no longer in the engineering or sre group",
            &"joined through the engineering group"
        ])
    );
    Ok(())
}
