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
            allow: vec![],
            log: None,
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
             printf '%s' '{{\"type\":\"result\",\"is_error\":false,\"total_cost_usd\":0.25,\"result\":\"fine\"}}'",
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
            "pwd -P > '{}'\nprintf '%s\\n' \"$@\" > '{}'\nprintf '%s' '{{\"type\":\"result\",\"is_error\":false}}'",
            seen.display(),
            args.display()
        );
        let tools = SystemTools::with_claude(script(dir.path(), &body)?);
        let run_dir = dir.path().join("root").join("ada").join("work").join("k3x9a");

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
            contains_substring("keeps this directory for one Upheap item")
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
    async fn extra_allow_rules_are_values_of_allowed_tools() -> googletest::Result<()> {
        // Arrange: the script records its arguments, one a line.
        let dir = tempfile::tempdir()?;
        let args = dir.path().join("args");
        let body = format!(
            "printf '%s\\n' \"$@\" > '{}'\nprintf '%s' '{{\"type\":\"result\",\"is_error\":false}}'",
            args.display()
        );
        let tools = SystemTools::with_claude(script(dir.path(), &body)?);

        // Act
        let report = tools
            .run(ToolStart {
                allow: vec!["mcp__claude_ai_Claude_Docs".to_owned()],
                ..start(Access::Do)
            })
            .await;

        // Assert
        verify_that!(report?.failure, none())?;
        let seen = std::fs::read_to_string(&args)?;
        let lines: Vec<&str> = seen.lines().collect();
        let tool_rules = lines.iter().position(|line| *line == "--allowedTools");
        let rule = lines.iter().position(|line| *line == "mcp__claude_ai_Claude_Docs");
        verify_that!(tool_rules.zip(rule).map(|(first, rule)| first < rule), some(eq(true)))
    }

    #[gtest]
    #[tokio::test]
    async fn a_directory_that_is_there_already_is_reused_with_its_files_and_a_fresh_claude_md() -> googletest::Result<()>
    {
        // Arrange: an earlier run left a file and its own instructions.
        let dir = tempfile::tempdir()?;
        let seen = dir.path().join("seen");
        let tools = SystemTools::with_claude(script(
            dir.path(),
            &format!(
                "pwd -P > '{}'\nprintf '%s' '{{\"type\":\"result\",\"is_error\":false}}'",
                seen.display()
            ),
        )?);
        let run_dir = dir.path().join("taken");
        std::fs::create_dir(&run_dir)?;
        std::fs::write(run_dir.join("left.txt"), "what the last run left")?;
        std::fs::write(run_dir.join("CLAUDE.md"), "old instructions")?;

        // Act
        let result = tools
            .run(ToolStart {
                dir: Some(run_dir.clone()),
                ..start(Access::Do)
            })
            .await;

        // Assert
        verify_that!(result?.failure, none())?;
        verify_that!(
            PathBuf::from(std::fs::read_to_string(&seen)?.trim_end()),
            eq(&run_dir.canonicalize()?)
        )?;
        verify_that!(
            std::fs::read_to_string(run_dir.join("left.txt"))?,
            eq("what the last run left")
        )?;
        verify_that!(
            std::fs::read_to_string(run_dir.join("CLAUDE.md"))?,
            eq(claude::WORKSPACE_INSTRUCTIONS)
        )
    }

    #[gtest]
    #[tokio::test]
    async fn a_file_where_the_directory_should_be_is_no_directory_and_the_script_never_starts() -> googletest::Result<()>
    {
        // Arrange
        let dir = tempfile::tempdir()?;
        let started = dir.path().join("started");
        let tools = SystemTools::with_claude(script(dir.path(), &format!("echo > '{}'", started.display()))?);
        let in_the_way = dir.path().join("taken");
        std::fs::write(&in_the_way, "a file")?;

        // Act
        let result = tools
            .run(ToolStart {
                dir: Some(in_the_way.clone()),
                ..start(Access::Do)
            })
            .await;

        // Assert
        verify_that!(result, err(matches_pattern!(ToolError::NoDirectory { .. })))?;
        verify_that!(std::fs::read_to_string(&in_the_way)?, eq("a file"))?;
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

    #[gtest]
    #[tokio::test]
    async fn settled_waits_until_a_stopped_tool_that_ignores_the_request_is_gone() -> googletest::Result<()> {
        // Arrange: a script that ignores being asked to end, noting its process number, and is running.
        let dir = tempfile::tempdir()?;
        let pid_file = dir.path().join("pid");
        let body = format!(
            "trap '' TERM\necho $$ > '{}'\nwhile true; do sleep 0.05; done",
            pid_file.display()
        );
        let tools = SystemTools::with_claude(script(dir.path(), &body)?);
        let mut running = Box::pin(tools.run(start(Access::Do)));
        let started = tokio::time::timeout(Duration::from_secs(10), async {
            tokio::select! {
                _ = &mut running => {}
                () = async {
                    while !pid_file.exists() {
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                } => {}
            }
        })
        .await;
        let pid = std::fs::read_to_string(&pid_file)?.trim().to_owned();

        // Act: stop it, and wait for it to settle.
        drop(running);
        let before = std::time::Instant::now();
        let settled = tokio::time::timeout(Duration::from_secs(10), tools.settled()).await;
        let took = before.elapsed();
        let alive = std::process::Command::new("kill")
            .args(["-0", &pid])
            .output()
            .is_ok_and(|o| o.status.success());

        // Assert: it waited for the kill after the grace, and the process is gone.
        verify_that!(started.is_ok(), eq(true))?;
        verify_that!(settled.is_ok(), eq(true))?;
        verify_that!(took, ge(claude::STOP_GRACE / 2))?;
        verify_that!(alive, eq(false))
    }

    /// Whether the process `pid` is still there.
    fn is_alive(pid: &str) -> bool {
        std::process::Command::new("kill")
            .args(["-0", pid])
            .output()
            .is_ok_and(|o| o.status.success())
    }

    const RESULT: &str = r#"{"type":"result","is_error":false,"total_cost_usd":0.25,"result":"fine"}"#;

    #[gtest]
    #[tokio::test]
    async fn the_stream_is_saved_line_for_line_and_the_result_event_makes_the_report() -> googletest::Result<()> {
        // Arrange: three events, the last one without a newline, and a log in a directory that isn't there yet.
        let dir = tempfile::tempdir()?;
        let events = [
            r#"{"type":"system","subtype":"init"}"#,
            r#"{"type":"assistant","n":1}"#,
            RESULT,
        ];
        let body = format!(
            "echo '{}'\necho '{}'\nprintf '%s' '{}'",
            events[0], events[1], events[2]
        );
        let tools = SystemTools::with_claude(script(dir.path(), &body)?);
        let log = dir.path().join("config").join("runs").join("the-run.jsonl");

        // Act
        let report = tools
            .run(ToolStart {
                log: Some(log.clone()),
                ..start(Access::Do)
            })
            .await?;

        // Assert
        verify_that!(report.cost_usd, eq(Some(0.25)))?;
        verify_that!(report.failure, none())?;
        verify_that!(report.last_message.as_deref(), eq(Some("fine")))?;
        verify_that!(std::fs::read_to_string(&log)?, eq(&format!("{}\n", events.join("\n"))))
    }

    #[gtest]
    #[tokio::test]
    async fn a_script_that_goes_silent_is_stopped_with_what_it_started_and_the_run_fails() -> googletest::Result<()> {
        // Arrange: one event, then a command of the script's own that would run for a long time.
        let dir = tempfile::tempdir()?;
        let pids = dir.path().join("pids");
        let body = format!(
            "echo '{{\"type\":\"system\"}}'\necho $$ > '{pids}'\nsleep 30 &\necho $! >> '{pids}'\nwait",
            pids = pids.display()
        );
        let tools = SystemTools::with_claude(script(dir.path(), &body)?).with_silence_limit(Duration::from_secs(3));

        // Act
        let before = std::time::Instant::now();
        let report = tools.run(start(Access::Do)).await?;
        let took = before.elapsed();

        // Assert: it returned soon after the limit, with the failure, and nothing of the script is left.
        verify_that!(
            report.failure.as_deref(),
            eq(Some("The agent showed no activity for 3 seconds."))
        )?;
        verify_that!(report.cost_usd, none())?;
        verify_that!(report.last_message, none())?;
        verify_that!(took, lt(Duration::from_secs(8)))?;
        for pid in std::fs::read_to_string(&pids)?.split_whitespace() {
            verify_that!(is_alive(pid), eq(false))?;
        }
        Ok(())
    }

    #[gtest]
    #[tokio::test]
    async fn a_script_that_ignores_the_request_to_end_is_gone_when_the_silent_run_returns() -> googletest::Result<()> {
        // Arrange
        let dir = tempfile::tempdir()?;
        let pid_file = dir.path().join("pid");
        let body = format!(
            "trap '' TERM\necho $$ > '{}'\nwhile true; do sleep 0.05; done",
            pid_file.display()
        );
        let tools = SystemTools::with_claude(script(dir.path(), &body)?).with_silence_limit(Duration::from_secs(2));

        // Act
        let before = std::time::Instant::now();
        let report = tools.run(start(Access::Do)).await?;
        let took = before.elapsed();

        // Assert: killed after the grace, and gone by the time the run returned.
        verify_that!(
            report.failure.as_deref(),
            eq(Some("The agent showed no activity for 2 seconds."))
        )?;
        verify_that!(took, ge(claude::STOP_GRACE))?;
        verify_that!(is_alive(std::fs::read_to_string(&pid_file)?.trim()), eq(false))
    }

    #[gtest]
    #[tokio::test]
    async fn a_script_that_printed_its_result_and_then_hangs_is_stopped_and_reported_as_having_ended()
    -> googletest::Result<()> {
        let dir = tempfile::tempdir()?;
        let tools = SystemTools::with_claude(script(dir.path(), &format!("echo '{RESULT}'\nsleep 30"))?)
            .with_silence_limit(Duration::from_secs(3));

        let report = tools.run(start(Access::Do)).await?;

        verify_that!(report.failure, none())?;
        verify_that!(report.cost_usd, eq(Some(0.25)))
    }

    #[gtest]
    #[tokio::test]
    async fn a_script_that_prints_often_enough_is_not_silent() -> googletest::Result<()> {
        // Arrange: 4 seconds of output, with never a second between two lines, and a limit of 3 seconds. The limit is
        // generous because a script can take a while to start when many run at once.
        let dir = tempfile::tempdir()?;
        let body =
            format!("for i in 1 2 3 4 5 6 7 8; do echo '{{\"type\":\"assistant\"}}'; sleep 0.5; done\necho '{RESULT}'");
        let tools = SystemTools::with_claude(script(dir.path(), &body)?).with_silence_limit(Duration::from_secs(3));

        // Act
        let report = tools.run(start(Access::Do)).await?;

        // Assert
        verify_that!(report.failure, none())?;
        verify_that!(report.cost_usd, eq(Some(0.25)))
    }

    #[gtest]
    #[tokio::test]
    async fn a_script_that_only_writes_to_stderr_is_not_silent() -> googletest::Result<()> {
        let dir = tempfile::tempdir()?;
        let body = format!("for i in 1 2 3 4 5 6 7 8; do echo working >&2; sleep 0.5; done\necho '{RESULT}'");
        let tools = SystemTools::with_claude(script(dir.path(), &body)?).with_silence_limit(Duration::from_secs(3));

        let report = tools.run(start(Access::Do)).await?;

        verify_that!(report.failure, none())?;
        verify_that!(report.cost_usd, eq(Some(0.25)))
    }

    #[gtest]
    #[tokio::test]
    async fn a_log_that_cannot_be_made_does_not_fail_the_run() -> googletest::Result<()> {
        // Arrange: the log's directory would be under a file.
        let dir = tempfile::tempdir()?;
        let in_the_way = dir.path().join("file");
        std::fs::write(&in_the_way, "a file")?;
        let tools = SystemTools::with_claude(script(dir.path(), &format!("echo '{RESULT}'"))?);

        // Act
        let report = tools
            .run(ToolStart {
                log: Some(in_the_way.join("runs").join("the-run.jsonl")),
                ..start(Access::Do)
            })
            .await?;

        // Assert
        verify_that!(report.failure, none())?;
        verify_that!(report.cost_usd, eq(Some(0.25)))
    }

    #[gtest]
    #[tokio::test]
    async fn settled_with_nothing_stopped_returns_at_once() -> googletest::Result<()> {
        let tools = SystemTools::new();

        let settled = tokio::time::timeout(Duration::from_millis(500), tools.settled()).await;

        verify_that!(settled.is_ok(), eq(true))
    }
}
