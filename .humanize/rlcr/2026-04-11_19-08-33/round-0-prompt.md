Read and execute below with ultrathink

## Goal Tracker Setup (REQUIRED FIRST STEP)

Before starting implementation, you MUST initialize the Goal Tracker:

1. Read @/app/workspaces/4a2ab07b-4e45-40e5-9bae-086ee16bbd30/.humanize/rlcr/2026-04-11_19-08-33/goal-tracker.md
2. If the "Ultimate Goal" section says "[To be extracted...]", extract a clear goal statement from the plan
3. If the "Acceptance Criteria" section says "[To be defined...]", define 3-7 specific, testable criteria
4. Populate the "Active Tasks" table with tasks from the plan, mapping each to an AC and filling Tag/Owner
5. Write the updated goal-tracker.md

**IMPORTANT**: The IMMUTABLE SECTION can only be modified in Round 0. After this round, it becomes read-only.

---

## Implementation Plan

For all tasks that need to be completed, please use the Task system (TaskCreate, TaskUpdate, TaskList) to track each item in order of importance.
You are strictly prohibited from only addressing the most important issues - you MUST create Tasks for ALL discovered issues and attempt to resolve each one.

## Task Tag Routing (MUST FOLLOW)

Each task must have one routing tag from the plan: `coding` or `analyze`.

- Tag `coding`: Claude executes the task directly.
- Tag `analyze`: Claude must execute via `/humanize:ask-codex`, then integrate Codex output.
- Keep Goal Tracker "Active Tasks" columns **Tag** and **Owner** aligned with execution (`coding -> claude`, `analyze -> codex`).
- If a task has no explicit tag, default to `coding` (Claude executes directly).

# Implementation Plan: forgecode Go Port

## Goal
将 Rust 实现的 AI 编程助手工具 forgecode 的核心功能用 Go 语言重新实现。
新代码放在 go-impl/ 目录下，在 go-port 分支上完成所有提交。

## Acceptance Criteria

- AC-1: 创建 go-impl/ 目录，包含完整的 Go 模块 (go.mod, go.sum)
- AC-2: 实现 go-impl/cmd/forge/main.go 入口，使用 cobra CLI 框架
- AC-3: 实现 go-impl/internal/api/claude.go，能调用 Claude API 进行对话
- AC-4: 实现 go-impl/internal/agent/agent.go，核心 agent 对话逻辑
- AC-5: 实现 go-impl/internal/tools/file.go，文件读写工具
- AC-6: 添加 go-impl/README.md，说明 Go 版本使用方法

## Implementation Tasks

### Phase 1: Project Setup
- task1: 在 go-impl/ 目录创建 go.mod (module github.com/pp5ee/forgecode-go), go.sum
- task2: 添加依赖 github.com/spf13/cobra 做 CLI 框架

### Phase 2: Core Implementation
- task3: 实现 go-impl/internal/api/claude.go — Claude API HTTP 客户端
- task4: 实现 go-impl/internal/agent/agent.go — 对话 agent 逻辑
- task5: 实现 go-impl/internal/tools/file.go — 文件读写工具
- task6: 实现 go-impl/cmd/forge/main.go — CLI 入口 (cobra root command)

### Phase 3: Documentation
- task7: 编写 go-impl/README.md，说明 Go 版本的构建和使用

---

## BitLesson Selection (REQUIRED FOR EACH TASK)

Before executing each task or sub-task, you MUST:

1. Read @/app/workspaces/4a2ab07b-4e45-40e5-9bae-086ee16bbd30/.humanize/bitlesson.md
2. Run `bitlesson-selector` for each task/sub-task to select relevant lesson IDs
3. Follow the selected lesson IDs (or `NONE`) during implementation

Include a `## BitLesson Delta` section in your summary with:
- Action: none|add|update
- Lesson ID(s): NONE or comma-separated IDs
- Notes: what changed and why (required if action is add or update)

Reference: @/app/workspaces/4a2ab07b-4e45-40e5-9bae-086ee16bbd30/.humanize/bitlesson.md

---

## Goal Tracker Rules

Throughout your work, you MUST maintain the Goal Tracker:

1. **Before starting a task**: Mark it as "in_progress" in Active Tasks
   - Confirm Tag/Owner routing is correct before execution
2. **After completing a task**: Move it to "Completed and Verified" with evidence (but mark as "pending verification")
3. **If you discover the plan has errors**:
   - Do NOT silently change direction
   - Add entry to "Plan Evolution Log" with justification
   - Explain how the change still serves the Ultimate Goal
4. **If you need to defer a task**:
   - Move it to "Explicitly Deferred" section
   - Provide strong justification
   - Explain impact on Acceptance Criteria
5. **If you discover new issues**: Add to "Open Issues" table

---

Note: You MUST NOT try to exit `start-rlcr-loop` loop by lying or edit loop state file or try to execute `cancel-rlcr-loop`

After completing the work, please:
0. If you have access to the `code-simplifier` agent, use it to review and optimize the code you just wrote
1. Finalize @/app/workspaces/4a2ab07b-4e45-40e5-9bae-086ee16bbd30/.humanize/rlcr/2026-04-11_19-08-33/goal-tracker.md (this is Round 0, so you are initializing it - see "Goal Tracker Setup" above)
2. Commit your changes with a descriptive commit message
3. Write your work summary into @/app/workspaces/4a2ab07b-4e45-40e5-9bae-086ee16bbd30/.humanize/rlcr/2026-04-11_19-08-33/round-0-summary.md
