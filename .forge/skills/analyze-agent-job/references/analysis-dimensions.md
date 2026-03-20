# Analysis Dimensions

Checklist of gap-analysis dimensions to evaluate for every job. Not all dimensions apply to every task -- skip dimensions that are clearly irrelevant.

**Critical constraint**: The agent being analyzed NEVER has access to verifier tests. The verifier runs after the agent finishes in a separate container. When evaluating gaps, always distinguish between:
- **Agent-attributable gaps**: things the agent could have done better given ONLY the user prompt, its tools, and common conventions
- **Hidden verifier assumptions**: expectations the agent could not have reasonably known about -- classify these as verifier misalignment, not agent failures

## 1. Discovery Gaps

**Question**: Did the agent find everything that could be reasonably expected from the user prompt?

How to check:
- Extract all file paths, values, and patterns from verifier test traces (ctrf.json)
- For each: was it explicitly mentioned in the user prompt, or inferable from common conventions?
- Search dump.json for each expected path/value
- Identify which expected items the agent never searched for or never found
- Classify each gap as: **explicit requirement missed**, **convention missed**, or **hidden assumption**

Common patterns:
- Agent found 2 of 3 contaminated files (prompt said "all")
- Agent found one token format but missed a variant
- Agent searched broadly but ignored results from certain file types (e.g., .json artifacts)
- Agent implemented correct logic but with wrong API signature that doesn't match common calling conventions

## 2. Tool Usage Errors

**Question**: Did the agent use the right tools correctly?

Check for:
- Using `shell` with `grep`/`rg` instead of `fs_search`
- Using `write` when `patch` would be more appropriate
- Calling tools with overly broad or overly narrow parameters
- Not using available tools at all (e.g., never using `sem_search` when it would help)
- Using tools for communication instead of actual work

## 3. Search Strategy Failures

**Question**: Was the agent's search strategy exhaustive enough for the task?

Critical checks:
- **Truncation handling**: Search for "truncated", "INCOMPLETE", "exceeding" in tool results. Did the agent follow up on truncated results?
- **Exact-value follow-up**: After discovering a secret value, did the agent search for that exact value across all files?
- **Pattern coverage**: Did the agent search for all relevant patterns (e.g., all token formats, not just one)?
- **File type coverage**: Did the agent search across all file types, or only common ones (.py, .yaml) while missing others (.json, .toml)?
- **Glob validity**: Did the agent use valid glob syntax? (e.g., `*.{py,yaml}` may not work in all tools)

## 4. Over-action (Destructive Changes)

**Question**: Did the agent do more than what was asked?

Check for:
- Git history rewriting (filter-branch, rebase, force-push) when only working-tree edits were needed
- Deleting files that weren't contaminated
- Modifying files that didn't need changes
- Installing packages or changing system state unnecessarily
- Creating files that weren't requested

Key rule: the changed-file set should be a subset of the contaminated/relevant-file set.

## 5. Under-action (Missing Steps)

**Question**: Did the agent skip steps that would have been necessary?

Check for:
- Not verifying changes after making them (no post-edit search/grep)
- Not testing alternative calling conventions / API usage patterns that external callers might use
- Not consulting documentation (via fetch/context7/deepwiki) for correct API conventions when implementing in an unfamiliar language or domain
- Declaring completion without confirmation
- Skipping edge cases mentioned in the user prompt
- Not designing for robustness: accepting only one rigid input format when the task description or language conventions suggest multiple valid patterns
- Not re-reading the user prompt carefully for signal words like "primary input" that imply parameter ordering or API design

## 6. Prompt Conflicts

**Question**: Are there tensions in the prompt partials that could confuse the agent?

Read these files and look for conflicting guidance:
- `crates/forge_repo/src/agents/forge.md` -- main agent prompt
- `crates/forge_repo/src/agents/explorer.md` -- delegated explorer prompt
- `templates/forge-partial-background-instructions.md` -- background mode instructions
- `templates/forge-partial-system-info.md` -- environment info

Common conflict patterns:
- "Use delegation for exploration" vs tasks requiring exhaustive audit
- "Fast exploration" (explorer.md) vs "ensure all occurrences are found" (task requirement)
- "TDD development" (background instructions) vs data/config cleanup tasks where TDD is wrong abstraction
- "Minimize state changes" (background instructions) with no explicit ban on VCS history rewrite
- "Verifier-first" guidance existing but not being dominant enough vs "delegation-first" patterns

## 7. Capability Gaps

**Question**: Would a different or additional tool capability have helped?

Evaluate honestly:
- Most failures are **strategy errors**, not capability gaps
- The agent typically has search, read, edit, shell, delegation -- enough for most tasks
- If a capability gap exists, describe it generically (e.g., "baseline diff comparison helper")
- Distinguish between "would be nice" and "was actually necessary"

## 8. Verifier Misalignment (Hidden Assumptions)

**Question**: Were there hidden verifier assumptions the agent couldn't reasonably know?

This is the most important dimension for fair analysis. The agent NEVER sees the verifier tests. Every gap must be classified:

- **Explicit**: The user prompt clearly stated the requirement. Agent should have followed it. **Agent's fault.**
- **Conventional**: The requirement follows well-known conventions in the language/domain (e.g., R functions typically accept domain as a vector, Python functions follow PEP conventions). Agent should have known or looked up docs. **Agent's fault, but softer.**
- **Hidden**: The verifier has an expectation that is neither in the prompt nor inferable from conventions (e.g., specific file paths, exact error message wording, obscure API calling patterns). **Not the agent's fault -- classify as verifier misalignment.**

Check for:
- Verifier expecting specific commit SHAs to exist (agent rewrote history)
- Verifier expecting exact replacement strings (agent used different placeholders)
- Verifier checking files the user prompt didn't mention
- Verifier running in a different working directory than expected
- Verifier calling the function with a different signature than what the prompt implies
- Verifier expecting specific argument names or parameter ordering not mentioned in the prompt
- Time-dependent or environment-dependent test assertions

When a failure is classified as hidden/verifier misalignment, the recommendation should be about making the agent more robust and convention-aware, NOT about reading the verifier.

## 9. Delegation Quality

**Question**: If the agent delegated work, was the delegation effective?

Check for:
- Explorer returning a summary that was treated as exhaustive without verification
- Delegated agent not having enough context about the task requirements
- Parent agent not validating delegated results before acting on them
- Delegation to "fast" exploration mode when "exhaustive" was needed

## 10. Test Isolation Validity

**Question**: Were the agent's tests structurally capable of catching the actual failure?

Check for:
- Agent mocked or patched a fundamental primitive (process identity/rank, distributed init, network, filesystem, clock) and never tested with the real thing
- Agent's tests passed but the mock was not a faithful substitute — the real caller uses the real primitive, not the mock
- Agent simulated a distributed/concurrent/multi-process behavior in a single-process environment, which can pass even when the real cross-process contract is broken
- Agent's test setup was architecturally different from the verifier's test setup (e.g., agent used monkeypatch, verifier used `mp.spawn`)

This is distinct from "test coverage gaps" — the tests may cover every code path, but if the test harness itself is a broken substitute for the real execution environment, all tests are structurally invalid.

When this gap is present, the root cause is: **the agent validated its own assumptions, not the real contract**.

Common examples:
- Patching `get_rank()` in-process instead of using `torch.multiprocessing.spawn`
- Mocking HTTP responses instead of testing against a real local server
- Using in-memory fakes for filesystem operations when the real caller uses actual disk paths
- Simulating async behavior with synchronous stubs

## 11. Completion Verification

**Question**: Did the agent verify its work met the requirements?

Check for:
- Final verification search after all edits
- Checking that only contaminated files were modified
- Comparing the changed-file set against the discovered-contamination set
- Running available tests or verifier before declaring done
- Treating "no matches" in a verification search as proof of completion (could be a bad search pattern)

## 12. Shortcut / Spirit-vs-Letter Violation

**Question**: Did the agent satisfy the letter of a constraint while violating its intent?

This is distinct from the agent simply failing — the agent may have found a technically valid workaround that:
1. Passes its own tests in the development environment
2. Fails under the real harness because the harness specifically tests for the intended behavior

Check for:
- Agent explicitly reasoned about whether a shortcut was "allowed" and chose to proceed
- Solution's artifact size is implausibly small for the stated task complexity (e.g., a 100-byte C file for a rendering algorithm)
- Solution depends on files being present in the working directory that will not exist in an isolated harness
- Solution uses filesystem operations (hard links, symlinks, copies) instead of computation when the task says "generate algorithmically"
- Solution reads a file indirectly (via link, copy, exec) when the task says "do not read input file X"

Key diagnostic questions:
- Does the agent's binary/script work in a blank temporary directory with no supporting files?
- Does the compressed artifact size match the expected complexity of the task?
- Did the agent explicitly reason about whether a shortcut was ethical/permitted and proceed anyway?

When this gap is present, the correct recommendation is: **honor the spirit of the constraint, not just the letter. Verify the solution works from a blank slate.**

## Severity Classification

- **Critical**: Directly caused test failure (e.g., missed a contaminated file, rewrote git history, shortcut broke under harness)
- **Moderate**: Contributed to failure or created risk (e.g., ignored truncation warning, didn't verify)
- **Low**: Suboptimal but didn't cause failure (e.g., unnecessary delegation, verbose approach)
