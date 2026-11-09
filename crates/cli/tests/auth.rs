//! `upheap auth login` and `auth logout`, and the tokens every command uses: kept, refreshed and revoked.

mod common;

use anyhow::{Context, Result};
use cli::{StoredTokens, TokenStore};
use common::{ConsentBrowser, TestCli, TestServer};
use googletest::prelude::*;

#[gtest]
#[tokio::test]
async fn login_signs_in_through_the_browser_and_keeps_the_server() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let ada = server.register("ada", "Ada Lovelace").await?;
    let mut cli = TestCli::new(&server)?.with_browser(ConsentBrowser {
        server: server.url.clone(),
        session: ada.session.clone(),
        allow: true,
    });
    cli.env.server = None;

    // Act
    let login = cli
        .run(&["auth", "login", "--server", &format!("{}/", server.url)])
        .await;
    let heaps = cli.run(&["heap", "list", "--quiet"]).await;

    // Assert
    expect_that!(
        login.stderr,
        contains_substring(format!("{}/oauth/authorize?", server.url))
    );
    expect_that!(
        login.stdout,
        eq(&format!("Signed in to {} as Ada Lovelace (@ada).\n", server.url))
    );
    expect_that!(login.exited(0), eq(true));
    expect_that!(
        cli.config_text(),
        contains_substring(format!("server = \"{}\"", server.url))
    );
    expect_that!(cli.stored(&server)?, some(anything()));
    expect_that!(heaps.stdout, eq("ada/personal\n"));
    expect_that!(heaps.exited(0), eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn cancelling_on_the_consent_page_signs_nothing_in() -> Result<()> {
    let server = TestServer::start().await?;
    let ada = server.register("ada", "Ada").await?;
    let cli = TestCli::new(&server)?.with_browser(ConsentBrowser {
        server: server.url.clone(),
        session: ada.session.clone(),
        allow: false,
    });

    let login = cli.run(&["auth", "login"]).await;

    expect_that!(
        login.stderr,
        contains_substring("you cancelled the sign-in in the browser")
    );
    expect_that!(login.exited(4), eq(true));
    expect_that!(cli.stored(&server)?, none());
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_command_without_a_sign_in_says_how_to_sign_in() -> Result<()> {
    let server = TestServer::start().await?;
    let cli = TestCli::new(&server)?;

    let heaps = cli.run(&["heap", "list"]).await;

    expect_that!(
        heaps.stderr,
        eq(&format!(
            "error: not signed in to {}\nhint: sign in with `upheap auth login`\n",
            server.url
        ))
    );
    expect_that!(heaps.exited(4), eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_stale_access_token_is_refreshed_and_the_rotated_tokens_are_kept() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let ada = server.register("ada", "Ada").await?;
    let cli = TestCli::signed_in(&server, &ada).await?;
    let first = cli.stored(&server)?.context("signed in")?;
    let stale = StoredTokens {
        access_expires_at: 0,
        ..first.clone()
    };
    cli.tokens.save(&server.url, &stale)?;

    // Act
    let heaps = cli.run(&["heap", "list", "--quiet"]).await;

    // Assert
    let kept = cli.stored(&server)?.context("still signed in")?;
    expect_that!(heaps.stdout, eq("ada/personal\n"));
    expect_that!(heaps.exited(0), eq(true));
    expect_that!(kept.refresh_token, not(eq(&first.refresh_token)));
    expect_that!(kept.access_token, not(eq(&first.access_token)));
    expect_that!(kept.has_fresh_access(jiff::Timestamp::now().as_second()), eq(true));
    let (status, _) = server
        .token_request(&[
            ("grant_type", "refresh_token"),
            ("client_id", "upheap-cli"),
            ("refresh_token", &kept.refresh_token),
        ])
        .await?;
    expect_that!(status, eq(200));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_fresh_access_token_is_used_without_refreshing() -> Result<()> {
    let server = TestServer::start().await?;
    let ada = server.register("ada", "Ada").await?;
    let cli = TestCli::signed_in(&server, &ada).await?;
    let before = cli.stored(&server)?;

    let heaps = cli.run(&["heap", "list", "--quiet"]).await;

    expect_that!(heaps.exited(0), eq(true));
    expect_that!(cli.stored(&server)?, eq(&before));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_refused_refresh_token_is_forgotten_and_reported_as_not_signed_in() -> Result<()> {
    // Arrange: the sign-in was revoked elsewhere, and the access token has gone stale.
    let server = TestServer::start().await?;
    let ada = server.register("ada", "Ada").await?;
    let cli = TestCli::signed_in(&server, &ada).await?;
    let stored = cli.stored(&server)?.context("signed in")?;
    let other = TestCli::new(&server)?;
    other.tokens.save(&server.url, &stored)?;
    expect_that!(other.run(&["auth", "logout"]).await.exited(0), eq(true));
    cli.tokens.save(
        &server.url,
        &StoredTokens {
            access_expires_at: 0,
            ..stored
        },
    )?;

    // Act
    let heaps = cli.run(&["heap", "list"]).await;

    // Assert
    expect_that!(heaps.stderr, contains_substring("not signed in"));
    expect_that!(heaps.exited(4), eq(true));
    expect_that!(cli.stored(&server)?, none());
    Ok(())
}

#[gtest]
#[tokio::test]
async fn logout_revokes_the_sign_in_on_the_server_and_forgets_it() -> Result<()> {
    // Arrange
    let server = TestServer::start().await?;
    let ada = server.register("ada", "Ada").await?;
    let cli = TestCli::signed_in(&server, &ada).await?;
    let stored = cli.stored(&server)?.context("signed in")?;

    // Act
    let logout = cli.run(&["auth", "logout"]).await;

    // Assert
    expect_that!(logout.stdout, eq(&format!("Signed out of {}.\n", server.url)));
    expect_that!(logout.exited(0), eq(true));
    expect_that!(cli.stored(&server)?, none());
    let (status, body) = server
        .token_request(&[
            ("grant_type", "refresh_token"),
            ("client_id", "upheap-cli"),
            ("refresh_token", &stored.refresh_token),
        ])
        .await?;
    expect_that!(status, eq(400));
    expect_that!(body["error"].as_str(), some(eq("invalid_grant")));
    expect_that!(server.me_status(&stored.access_token).await?, eq(401));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn logout_when_not_signed_in_has_nothing_to_do() -> Result<()> {
    let server = TestServer::start().await?;
    let cli = TestCli::new(&server)?;

    let logout = cli.run(&["auth", "logout"]).await;

    expect_that!(
        logout.stdout,
        eq(&format!("Not signed in to {}; nothing to do.\n", server.url))
    );
    expect_that!(logout.exited(0), eq(true));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn without_a_server_a_command_says_how_to_set_one() -> Result<()> {
    let server = TestServer::start().await?;
    let mut cli = TestCli::new(&server)?;
    cli.env.server = None;

    let heaps = cli.run(&["heap", "list"]).await;

    expect_that!(heaps.stderr, contains_substring("no server set"));
    expect_that!(heaps.exited(2), eq(true));
    Ok(())
}
