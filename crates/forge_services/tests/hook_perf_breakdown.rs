//! Temporary benchmark: measures individual phases of hook execution to
//! understand where time is spent.
//!
//! Run with:
//!   cargo test -p forge_services --test hook_perf_breakdown -- --nocapture

#[cfg(unix)]
mod bench {
    use std::collections::HashMap;
    use std::time::{Duration, Instant};

    use forge_app::{HookExecResult, HookOutcome};
    use forge_domain::{HookInput, HookInputBase, HookInputPayload, HookOutput, ShellHookCommand};
    use forge_services::ForgeShellHookExecutor;
    use futures::future::join_all;
    use serde_json::json;
    use tokio::io::AsyncWriteExt;
    use tokio::process::Command;

    /// Build a realistic HookInput (PreToolUse) for benchmarking.
    fn make_input() -> HookInput {
        let cwd = std::env::current_dir().unwrap();
        HookInput {
            base: HookInputBase {
                hook_event_name: "PreToolUse".to_string(),
                session_id: "bench-session".to_string(),
                transcript_path: cwd.join("transcript.jsonl"),
                cwd: cwd.clone(),
                permission_mode: None,
                agent_id: Some("agent-0".to_string()),
                agent_type: Some("code".to_string()),
            },
            payload: HookInputPayload::PreToolUse {
                tool_name: "Bash".to_string(),
                tool_input: json!({"command": "echo hello"}),
                tool_use_id: "bench-tool-use-id".to_string(),
            },
        }
    }

    /// Build the standard env vars map.
    fn make_env_vars() -> HashMap<String, String> {
        let cwd = std::env::current_dir().unwrap();
        let mut env = HashMap::new();
        env.insert("FORGE_PROJECT_DIR".to_string(), cwd.display().to_string());
        env.insert("FORGE_SESSION_ID".to_string(), "bench-session".to_string());
        env
    }

    /// Build a ShellHookCommand that reads stdin JSON and echoes a valid
    /// HookOutput.
    fn make_echo_hook_command() -> ShellHookCommand {
        ShellHookCommand {
            command: "read input && echo '{\"continue\": true}'".to_string(),
            condition: None,
            shell: None,
            timeout: None,
            status_message: None,
            once: false,
            async_mode: false,
            async_rewake: false,
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn hook_perf_breakdown() {
        const WARM: usize = 3;
        const RUNS: usize = 10;

        eprintln!("\n{}", "=".repeat(70));
        eprintln!("  Hook Performance Breakdown");
        eprintln!("{}\n", "=".repeat(70));

        // -----------------------------------------------------------
        // Phase 1: Single `bash -c 'exit 0'` (baseline spawn cost)
        // -----------------------------------------------------------
        {
            // warm up
            for _ in 0..WARM {
                let _ = Command::new("bash")
                    .args(["-c", "exit 0"])
                    .output()
                    .await
                    .unwrap();
            }

            let mut total = Duration::ZERO;
            for _ in 0..RUNS {
                let t = Instant::now();
                let _ = Command::new("bash")
                    .args(["-c", "exit 0"])
                    .output()
                    .await
                    .unwrap();
                total += t.elapsed();
            }
            let avg = total / RUNS as u32;
            eprintln!(
                "Phase 1  bare 'bash -c exit 0'              avg {avg:>10.3?}  (total {total:.3?} / {RUNS} runs)"
            );
        }

        // -----------------------------------------------------------
        // Phase 2: Single bash with stdin JSON pipe + stdout read
        // -----------------------------------------------------------
        {
            let json_payload = serde_json::to_string(&make_input()).unwrap();

            for _ in 0..WARM {
                let mut child = Command::new("bash")
                    .args(["-c", "read input && echo '{\"continue\": true}'"])
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .spawn()
                    .unwrap();
                let mut stdin = child.stdin.take().unwrap();
                stdin.write_all(json_payload.as_bytes()).await.unwrap();
                stdin.write_all(b"\n").await.unwrap();
                drop(stdin);
                let _ = child.wait_with_output().await.unwrap();
            }

            let mut total = Duration::ZERO;
            for _ in 0..RUNS {
                let t = Instant::now();
                let mut child = Command::new("bash")
                    .args(["-c", "read input && echo '{\"continue\": true}'"])
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .spawn()
                    .unwrap();
                let mut stdin = child.stdin.take().unwrap();
                stdin.write_all(json_payload.as_bytes()).await.unwrap();
                stdin.write_all(b"\n").await.unwrap();
                drop(stdin);
                let output = child.wait_with_output().await.unwrap();
                let _stdout = String::from_utf8_lossy(&output.stdout);
                total += t.elapsed();
            }
            let avg = total / RUNS as u32;
            eprintln!(
                "Phase 2  bash + stdin JSON + stdout read     avg {avg:>10.3?}  (total {total:.3?} / {RUNS} runs)"
            );
        }

        // -----------------------------------------------------------
        // Phase 3: serde_json::to_string serialization of HookInput
        // -----------------------------------------------------------
        {
            let input = make_input();

            // warm up
            for _ in 0..WARM {
                let _ = serde_json::to_string(&input).unwrap();
            }

            let iters = 10_000;
            let t = Instant::now();
            for _ in 0..iters {
                let _ = std::hint::black_box(serde_json::to_string(&input).unwrap());
            }
            let total = t.elapsed();
            let avg = total / iters as u32;
            let json_len = serde_json::to_string(&input).unwrap().len();
            eprintln!(
                "Phase 3  serde_json::to_string(HookInput)    avg {avg:>10.3?}  ({iters} iters, {json_len} bytes)"
            );
        }

        // -----------------------------------------------------------
        // Phase 4: serde_json::from_str::<HookOutput> parsing
        // -----------------------------------------------------------
        {
            let json_str = r#"{"continue": true, "decision": "approve", "reason": "looks good"}"#;

            // warm up
            for _ in 0..WARM {
                let _ = serde_json::from_str::<HookOutput>(json_str).unwrap();
            }

            let iters = 10_000;
            let t = Instant::now();
            for _ in 0..iters {
                let _ = std::hint::black_box(serde_json::from_str::<HookOutput>(json_str).unwrap());
            }
            let total = t.elapsed();
            let avg = total / iters as u32;
            eprintln!(
                "Phase 4  serde_json::from_str::<HookOutput>  avg {avg:>10.3?}  ({iters} iters, {} bytes input)",
                json_str.len()
            );
        }

        // -----------------------------------------------------------
        // Phase 5: Inline variable substitution (simulates
        //          substitute_variables which is not publicly exported)
        // -----------------------------------------------------------
        {
            let template = "echo ${FORGE_PROJECT_DIR} && ls ${FORGE_SESSION_ID}";
            let vars = make_env_vars();

            // A simple inline substitute matching the real impl's
            // behaviour (replace ${VAR} with env value).
            fn substitute(cmd: &str, vars: &HashMap<String, String>) -> String {
                let mut result = cmd.to_string();
                for (k, v) in vars {
                    result = result.replace(&format!("${{{k}}}"), v);
                }
                result
            }

            // warm up
            for _ in 0..WARM {
                let _ = substitute(template, &vars);
            }

            let iters = 100_000;
            let t = Instant::now();
            for _ in 0..iters {
                let _ = std::hint::black_box(substitute(template, &vars));
            }
            let total = t.elapsed();
            let avg = total / iters as u32;
            eprintln!(
                "Phase 5  substitute_variables (2 vars)       avg {avg:>10.3?}  ({iters} iters)"
            );
        }

        // -----------------------------------------------------------
        // Phase 6: 10× parallel bare `bash -c 'exit 0'` via join_all
        // -----------------------------------------------------------
        {
            // warm up
            for _ in 0..WARM {
                let futs: Vec<_> = (0..10)
                    .map(|_| Command::new("bash").args(["-c", "exit 0"]).output())
                    .collect();
                let _ = join_all(futs).await;
            }

            let mut total = Duration::ZERO;
            for _ in 0..RUNS {
                let t = Instant::now();
                let futs: Vec<_> = (0..10)
                    .map(|_| Command::new("bash").args(["-c", "exit 0"]).output())
                    .collect();
                let results = join_all(futs).await;
                total += t.elapsed();

                // Sanity check
                for r in &results {
                    assert!(r.as_ref().unwrap().status.success());
                }
            }
            let avg = total / RUNS as u32;
            eprintln!(
                "Phase 6  10× parallel 'bash -c exit 0'       avg {avg:>10.3?}  (total {total:.3?} / {RUNS} runs)"
            );
        }

        // -----------------------------------------------------------
        // Phase 7: 10× parallel full pipeline via
        //          ForgeShellHookExecutor::execute()
        // -----------------------------------------------------------
        {
            let executor = ForgeShellHookExecutor::new();
            let input = make_input();
            let env_vars = make_env_vars();
            let config = make_echo_hook_command();

            // warm up
            for _ in 0..WARM {
                let futs: Vec<_> = (0..10)
                    .map(|_| executor.execute(&config, &input, env_vars.clone(), None))
                    .collect();
                let _ = join_all(futs).await;
            }

            let mut total = Duration::ZERO;
            for _ in 0..RUNS {
                let t = Instant::now();
                let futs: Vec<_> = (0..10)
                    .map(|_| executor.execute(&config, &input, env_vars.clone(), None))
                    .collect();
                let results: Vec<anyhow::Result<HookExecResult>> = join_all(futs).await;
                total += t.elapsed();

                // Verify correctness
                for (i, r) in results.iter().enumerate() {
                    let r = r
                        .as_ref()
                        .unwrap_or_else(|e| panic!("hook {i} failed: {e}"));
                    assert_eq!(r.outcome, HookOutcome::Success, "hook {i} not Success");
                    assert!(r.output.is_some(), "hook {i} missing output");
                    assert_eq!(r.exit_code, Some(0), "hook {i} non-zero exit");
                }
            }
            let avg = total / RUNS as u32;
            eprintln!(
                "Phase 7  10× ForgeShellHookExecutor          avg {avg:>10.3?}  (total {total:.3?} / {RUNS} runs)"
            );
        }

        // -----------------------------------------------------------
        // Summary
        // -----------------------------------------------------------
        eprintln!("\n{}", "=".repeat(70));
        eprintln!("  Done. Overhead = Phase7 - Phase6 shows executor overhead");
        eprintln!("  per batch. Phase2 - Phase1 shows IO piping cost per call.");
        eprintln!("{}\n", "=".repeat(70));
    }
}
