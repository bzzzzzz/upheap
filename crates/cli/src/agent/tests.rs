use googletest::prelude::*;

use super::*;

#[gtest]
fn a_program_is_found_on_the_path_only_when_it_is_runnable() -> googletest::Result<()> {
    let dir = tempfile::tempdir()?;
    let other = tempfile::tempdir()?;
    let program = dir.path().join("claude");
    std::fs::write(&program, "#!/bin/sh\n")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // Not executable yet: a file called claude is not a tool.
        let path = std::env::join_paths([other.path(), dir.path()])?;
        verify_that!(find_on_path("claude", Some(path.clone())), none())?;
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755))?;
        verify_that!(find_on_path("claude", Some(path)), some(eq(&program)))?;
    }
    verify_that!(find_on_path("claude", None), none())?;
    verify_that!(find_on_path("nothing-by-this-name", Some(dir.path().into())), none())
}

#[gtest]
fn a_tool_that_is_not_there_is_not_installed() {
    let tools = SystemTools::with_claude(PathBuf::from("/definitely/not/here/claude"));

    expect_that!(
        tools.check(Tool::Claude),
        err(matches_pattern!(ToolError::NotInstalled { .. }))
    );
}

#[cfg(unix)]
mod processes {
    use std::os::unix::fs::PermissionsExt;
    use std::time::Duration;

    use super::*;

    /// A script standing in for `claude`, in a directory of its own.
    fn script(dir: &Path, body: &str) -> std::io::Result<PathBuf> {
        let path = dir.join("claude");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n"))?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
        Ok(path)
    }

    fn start(access: Access) -> ToolStart {
        ToolStart {
            tool: Tool::Claude,
            access,
            brief: "The brief\nin two lines\n".to_owned(),
            env: vec![("UPHEAP_RUN".to_owned(), "the-run".to_owned())],
        }
    }

    #[gtest]
    #[tokio::test]
    async fn the_brief_arrives_on_stdin_the_environment_is_set_and_the_json_is_read() -> googletest::Result<()> {
        // Arrange: the script records what it got, then prints what claude prints.
        let dir = tempfile::tempdir()?;
        let seen = dir.path().join("seen");
        let body = format!(
            "cat > '{seen}.stdin'\nprintf '%s\\n' \"$UPHEAP_RUN\" \"$@\" > '{seen}.args'\nprintf '%s' \"$PATH\" > '{seen}.path'\n\
             printf '%s' '{{\"is_error\":false,\"total_cost_usd\":0.25,\"result\":\"fine\"}}'",
            seen = seen.display()
        );
        let tools = SystemTools::with_claude(script(dir.path(), &body)?);

        // Act
        let report = tools.run(start(Access::Prepare)).await;

        // Assert
        let report = report?;
        verify_that!(report.cost_usd, eq(Some(0.25)))?;
        verify_that!(report.failure, none())?;
        verify_that!(report.last_message.as_deref(), eq(Some("fine")))?;
        verify_that!(
            std::fs::read_to_string(dir.path().join("seen.stdin"))?,
            eq("The brief\nin two lines\n")
        )?;
        let args = std::fs::read_to_string(dir.path().join("seen.args"))?;
        let mut lines = args.lines();
        verify_that!(lines.next(), some(eq("the-run")))?;
        verify_that!(lines.next(), some(eq("-p")))?;
        verify_that!(args, contains_substring("dontAsk"))?;
        // This binary's directory comes first on PATH, so the agent's `upheap` is this one.
        let path = std::fs::read_to_string(dir.path().join("seen.path"))?;
        let this_binary_dir = std::env::current_exe()?.parent().map(Path::to_path_buf);
        verify_that!(std::env::split_paths(&path).next(), eq(&this_binary_dir))
    }

    #[gtest]
    #[tokio::test]
    async fn a_script_that_exits_with_an_error_fails_with_the_last_line_of_stderr() -> googletest::Result<()> {
        let dir = tempfile::tempdir()?;
        let tools = SystemTools::with_claude(script(dir.path(), "echo 'bad flag' >&2\nexit 2")?);

        let report = tools.run(start(Access::Do)).await?;

        verify_that!(
            report.failure.as_deref(),
            eq(Some("claude exited with code 2: bad flag"))
        )
    }

    #[gtest]
    #[tokio::test]
    async fn a_program_that_cannot_start_is_would_not_start() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        // Runnable when checked, gone when started.
        let path = script(dir.path(), "exit 0").expect("a script");
        let tools = SystemTools::with_claude(path.clone());
        std::fs::write(&path, "#!/no/such/interpreter\n").expect("rewrite");

        let result = tools.run(start(Access::Do)).await;

        expect_that!(result, err(matches_pattern!(ToolError::WouldNotStart { .. })));
    }

    #[gtest]
    #[tokio::test]
    async fn dropping_the_future_kills_the_script() -> googletest::Result<()> {
        // Arrange: the script says it started, waits a moment, and would say it finished.
        let dir = tempfile::tempdir()?;
        let started = dir.path().join("started");
        let finished = dir.path().join("finished");
        let body = format!(
            "echo started > '{}'\nsleep 0.4\necho finished > '{}'",
            started.display(),
            finished.display()
        );
        let tools = SystemTools::with_claude(script(dir.path(), &body)?);
        let mut running = tools.run(start(Access::Do));

        // Act: poll it until the script has started, then drop it.
        let waited = tokio::time::timeout(Duration::from_secs(10), async {
            tokio::select! {
                _ = &mut running => {}
                () = async {
                    while !started.exists() {
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                } => {}
            }
        })
        .await;
        drop(running);
        tokio::time::sleep(Duration::from_millis(700)).await;

        // Assert
        verify_that!(waited.is_ok(), eq(true))?;
        verify_that!(started.exists(), eq(true))?;
        verify_that!(finished.exists(), eq(false))
    }
}
