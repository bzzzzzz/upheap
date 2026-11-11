use std::path::Path;

use googletest::prelude::*;

use super::*;

/// A stand-in for `security`: a script that adds its arguments to a file beside it, writes its standard input to
/// another, prints `stdout`, and exits with `code`. Run through `sh`, so no test starts a file another one is still writing.
fn stand_in(dir: &Path, stdout: &str, code: i32) -> googletest::Result<SecurityTool> {
    let script = dir.join("security.sh");
    std::fs::write(
        &script,
        format!(
            "printf '%s\\n' \"$@\" >> '{dir}/args'\ncat > '{dir}/stdin'\nprintf '%s' '{stdout}'\necho 'it went wrong' >&2\nexit {code}\n",
            dir = dir.display()
        ),
    )?;
    Ok(SecurityTool {
        program: "/bin/sh".into(),
        leading: vec![script.into_os_string()],
    })
}

fn read(dir: &Path, name: &str) -> googletest::Result<String> {
    Ok(std::fs::read_to_string(dir.join(name))?)
}

#[gtest]
fn find_asks_for_the_entry_and_returns_what_the_tool_prints() -> googletest::Result<()> {
    // Arrange
    let dir = tempfile::tempdir()?;
    let tool = stand_in(dir.path(), "{\"a\":1}\n", 0)?;

    // Act
    let found = tool.find("upheap", "https://up.example.com");

    // Assert
    expect_that!(found, ok(some(eq("{\"a\":1}"))));
    expect_that!(
        read(dir.path(), "args")?,
        eq("find-generic-password\n-s\nupheap\n-a\nhttps://up.example.com\n-w\n")
    );
    Ok(())
}

#[gtest]
fn an_entry_that_is_not_there_is_none_to_find_and_fine_to_delete() -> googletest::Result<()> {
    // Arrange
    let dir = tempfile::tempdir()?;
    let tool = stand_in(dir.path(), "", 44)?;

    // Act
    let found = tool.find("upheap", "https://up.example.com");
    let deleted = tool.delete("upheap", "https://up.example.com");

    // Assert
    expect_that!(found, ok(none()));
    expect_that!(deleted, ok(anything()));
    expect_that!(
        read(dir.path(), "args")?,
        ends_with("delete-generic-password\n-s\nupheap\n-a\nhttps://up.example.com\n")
    );
    Ok(())
}

#[gtest]
fn add_sends_the_secret_on_standard_input_in_hexadecimal_and_never_as_an_argument() -> googletest::Result<()> {
    // Arrange
    let dir = tempfile::tempdir()?;
    let tool = stand_in(dir.path(), "", 0)?;

    // Act
    let added = tool.add("upheap", "https://up.example.com:8443", b"{\"t\":\"s3cret\"}");

    // Assert
    expect_that!(added, ok(anything()));
    expect_that!(
        read(dir.path(), "args")?,
        eq("delete-generic-password\n-s\nupheap\n-a\nhttps://up.example.com:8443\n-i\n")
    );
    expect_that!(
        read(dir.path(), "stdin")?,
        eq(
            "add-generic-password -U -s \"upheap\" -a \"https://up.example.com:8443\" -X 7b2274223a22733363726574227d\n"
        )
    );
    Ok(())
}

#[gtest]
fn a_failure_carries_the_exit_code_and_what_the_tool_said() -> googletest::Result<()> {
    // Arrange
    let dir = tempfile::tempdir()?;
    let tool = stand_in(dir.path(), "", 51)?;

    // Act
    let found = tool.find("upheap", "https://up.example.com");
    let added = tool.add("upheap", "https://up.example.com", b"x");

    // Assert
    expect_that!(
        found.map_err(|err| err.to_string()),
        err(eq(
            "`security find-generic-password` failed with exit code 51: it went wrong"
        ))
    );
    expect_that!(
        added.map_err(|err| err.to_string()),
        err(eq(
            "`security delete-generic-password` failed with exit code 51: it went wrong"
        ))
    );
    Ok(())
}

#[gtest]
fn a_tool_that_is_not_there_is_an_error() {
    // Arrange
    let tool = SecurityTool {
        program: "/no/such/security".into(),
        leading: Vec::new(),
    };

    // Act
    let found = tool.find("upheap", "https://up.example.com");

    // Assert
    expect_that!(found, err(matches_pattern!(SecurityToolError::Start { .. })));
}

#[gtest]
fn what_could_start_a_second_command_is_refused_before_the_tool_runs() -> googletest::Result<()> {
    // Arrange
    let dir = tempfile::tempdir()?;
    let tool = stand_in(dir.path(), "", 0)?;

    // Act
    let newline = tool.add("upheap", "https://up.example.com\ndelete-keychain", b"x");
    let quote = tool.add("upheap", "https://up.example.com\" -A", b"x");
    let backslash = tool.add("upheap", "https://up.example.com\\", b"x");
    let empty = tool.add("upheap", "", b"x");
    let long = tool.add("upheap", "https://up.example.com", &[b'x'; 1024]);

    // Assert
    for refused in [newline, quote, backslash, empty] {
        expect_that!(refused, err(matches_pattern!(SecurityToolError::Unquotable { .. })));
    }
    expect_that!(long, err(matches_pattern!(SecurityToolError::TooLong)));
    expect_that!(dir.path().join("args").exists(), eq(false));
    Ok(())
}

/// The real tool and the real login keychain, under a service name of its own. Run by hand on a Mac:
/// `cargo test -p cli -- --ignored the_real_keychain`.
#[cfg(target_os = "macos")]
#[gtest]
#[ignore = "writes to the login keychain of whoever runs it"]
fn the_real_keychain_keeps_reads_replaces_and_forgets_an_entry() {
    // Arrange
    let tool = SecurityTool::system();
    let (service, account) = ("upheap-test", "https://up.example.com:8443/a b");
    let _ = tool.delete(service, account);

    // Act
    let before = tool.find(service, account);
    let added = tool.add(service, account, b"{\"refresh_token\":\"one\"}");
    let first = tool.find(service, account);
    let replaced = tool.add(service, account, "{\"refresh_token\":\"zw\u{00e9}i\"}".as_bytes());
    let second = tool.find(service, account);
    let deleted = tool.delete(service, account);
    let after = tool.find(service, account);
    let deleted_again = tool.delete(service, account);

    // Assert
    expect_that!(before, ok(none()));
    expect_that!(added, ok(anything()));
    expect_that!(first, ok(some(eq("{\"refresh_token\":\"one\"}"))));
    expect_that!(replaced, ok(anything()));
    expect_that!(
        second.map(|text| text.and_then(|text| crate::auth::store::unhex(&text))),
        ok(some(eq(&"{\"refresh_token\":\"zw\u{00e9}i\"}".as_bytes().to_vec())))
    );
    expect_that!(deleted, ok(anything()));
    expect_that!(after, ok(none()));
    expect_that!(deleted_again, ok(anything()));
}
