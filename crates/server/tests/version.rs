//! `upheap-server --version` prints the binary's name and version, and starts no server.

use std::process::Command;

use googletest::prelude::*;

#[gtest]
fn version_prints_the_name_and_the_crate_version() -> anyhow::Result<()> {
    let expected = format!("upheap-server {}\n", env!("CARGO_PKG_VERSION"));
    for flag in ["--version", "-V"] {
        // Act
        let out = Command::new(env!("CARGO_BIN_EXE_upheap-server"))
            .arg(flag)
            .env_clear()
            .output()?;

        // Assert
        expect_that!(out.status.success(), eq(true), "{flag}");
        expect_that!(String::from_utf8(out.stdout)?, eq(&expected), "{flag}");
        expect_that!(String::from_utf8(out.stderr)?, eq(""), "{flag}");
    }
    Ok(())
}
