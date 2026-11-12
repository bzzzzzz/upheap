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
            dir: None,
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
    async fn with_a_directory_it_is_made_set_up_and_where_the_script_runs() -> googletest::Result<()> {
        // Arrange: the script records where it runs. The run's directory is under a root that isn't there yet.
        let dir = tempfile::tempdir()?;
        let seen = dir.path().join("seen");
        let args = dir.path().join("args");
        let body = format!(
            "pwd -P > '{}'\nprintf '%s\\n' \"$@\" > '{}'\nprintf '%s' '{{\"is_error\":false}}'",
            seen.display(),
            args.display()
        );
        let tools = SystemTools::with_claude(script(dir.path(), &body)?);
        let run_dir = dir.path().join("root").join("k3x9a-the-run");

        // Act
        let report = tools
            .run(ToolStart {
                dir: Some(run_dir.clone()),
                ..start(Access::Do)
            })
            .await;

        // Assert
        verify_that!(report?.failure, none())?;
        verify_that!(
            PathBuf::from(std::fs::read_to_string(&seen)?.trim_end()),
            eq(&run_dir.canonicalize()?)
        )?;
        verify_that!(
            std::fs::read_to_string(run_dir.join("CLAUDE.md"))?,
            contains_substring("for one run on one Upheap item")
        )?;
        // No settings file: Claude would ignore its allow rules in a directory nobody has trusted.
        verify_that!(run_dir.join(".claude").exists(), eq(false))?;
        verify_that!(
            std::fs::read_to_string(&args)?,
            contains_substring("--disallowedTools\nBash(upheap auth:*)")
        )
    }

    #[gtest]
    #[tokio::test]
    async fn a_directory_that_is_there_already_is_left_alone_and_the_script_never_starts() -> googletest::Result<()> {
        // Arrange
        let dir = tempfile::tempdir()?;
        let started = dir.path().join("started");
        let tools = SystemTools::with_claude(script(dir.path(), &format!("echo > '{}'", started.display()))?);
        let run_dir = dir.path().join("taken");
        std::fs::create_dir(&run_dir)?;

        // Act
        let result = tools
            .run(ToolStart {
                dir: Some(run_dir.clone()),
                ..start(Access::Do)
            })
            .await;

        // Assert
        verify_that!(result, err(matches_pattern!(ToolError::NoDirectory { .. })))?;
        verify_that!(std::fs::read_dir(&run_dir)?.count(), eq(0))?;
        verify_that!(started.exists(), eq(false))
    }

    #[gtest]
    #[tokio::test]
    async fn dropping_the_future_kills_the_script_and_what_it_started() -> googletest::Result<()> {
        // Arrange: the script starts a process of its own that would leave a file behind, says it started, waits a
        // moment, and would say it finished.
        let dir = tempfile::tempdir()?;
        let started = dir.path().join("started");
        let finished = dir.path().join("finished");
        let left_behind = dir.path().join("left-behind");
        let body = format!(
            "(sleep 0.4; echo alive > '{}') &\necho started > '{}'\nsleep 0.4\necho finished > '{}'",
            left_behind.display(),
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
        verify_that!(finished.exists(), eq(false))?;
        verify_that!(left_behind.exists(), eq(false))
    }

    #[gtest]
    #[tokio::test]
    async fn a_stopped_tool_is_asked_to_end_first_so_it_can_end_what_it_started() -> googletest::Result<()> {
        // Arrange: a script that, like Claude, cleans up when it is asked to end, and never ends on its own.
        let dir = tempfile::tempdir()?;
        let started = dir.path().join("started");
        let cleaned = dir.path().join("cleaned");
        let body = format!(
            "trap 'echo cleaned > \"{}\"; exit 0' TERM\necho started > '{}'\nwhile true; do sleep 0.05; done",
            cleaned.display(),
            started.display()
        );
        let tools = SystemTools::with_claude(script(dir.path(), &body)?);
        let mut running = tools.run(start(Access::Do));

        // Act: poll it until the script has started, then drop it. The drop returns at once; the script ends soon
        // after, on a thread of its own.
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
        let cleaned_in_time = tokio::time::timeout(Duration::from_secs(10), async {
            while !cleaned.exists() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await;

        // Assert
        verify_that!(waited.is_ok(), eq(true))?;
        verify_that!(cleaned_in_time.is_ok(), eq(true))
    }

    #[gtest]
    #[tokio::test]
    async fn stopped_tools_that_ignore_the_request_do_not_hold_up_the_thread_and_are_killed() -> googletest::Result<()>
    {
        // Arrange: two scripts that ignore being asked to end, each noting its process number.
        let mut dirs = Vec::new();
        let mut runs = Vec::new();
        for _ in 0..2 {
            let dir = tempfile::tempdir()?;
            let body = format!(
                "trap '' TERM\necho $$ > '{}'\nwhile true; do sleep 0.05; done",
                dir.path().join("pid").display()
            );
            let tools = SystemTools::with_claude(script(dir.path(), &body)?);
            runs.push(Box::pin(tools.run(start(Access::Do))));
            dirs.push(dir);
        }
        let started = tokio::time::timeout(Duration::from_secs(10), async {
            let mut polled = Box::pin(futures_util::future::join_all(runs.iter_mut()));
            tokio::select! {
                _ = &mut polled => {}
                () = async {
                    while !dirs.iter().all(|dir| dir.path().join("pid").exists()) {
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                } => {}
            }
        })
        .await;

        // Act: stop both together.
        let before = std::time::Instant::now();
        drop(runs);
        let took = before.elapsed();

        // Assert: the thread was free at once, and both were killed after the grace.
        verify_that!(started.is_ok(), eq(true))?;
        verify_that!(took, lt(claude::STOP_GRACE / 3))?;
        for dir in &dirs {
            let pid = std::fs::read_to_string(dir.path().join("pid"))?.trim().to_owned();
            let gone = tokio::time::timeout(Duration::from_secs(10), async {
                while std::process::Command::new("kill")
                    .args(["-0", &pid])
                    .output()
                    .is_ok_and(|o| o.status.success())
                {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            })
            .await;
            verify_that!(gone.is_ok(), eq(true))?;
        }
        Ok(())
    }
}
