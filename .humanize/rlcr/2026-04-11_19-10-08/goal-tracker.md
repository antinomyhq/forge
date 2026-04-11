# Goal Tracker

<!--
This file tracks the ultimate goal, acceptance criteria, and plan evolution.
It prevents goal drift by maintaining a persistent anchor across all rounds.

RULES:
- IMMUTABLE SECTION: Do not modify after initialization
- MUTABLE SECTION: Update each round, but document all changes
- Every task must be in one of: Active, Completed, or Deferred
- Deferred items require explicit justification
-->

## IMMUTABLE SECTION
<!-- Do not modify after initialization -->

### Ultimate Goal
将 Rust 实现的 AI 编程助手工具 forgecode 的核心功能用 Go 语言重新实现。
新代码放在 go-impl/ 目录下，在 go-port 分支上完成所有提交。

### Acceptance Criteria
<!-- Each criterion must be independently verifiable -->

- AC-1: 创建 go-impl/ 目录，包含完整的 Go 模块 (go.mod, go.sum)
- AC-2: 实现 go-impl/cmd/forge/main.go 入口，使用 cobra CLI 框架
- AC-3: 实现 go-impl/internal/api/claude.go，能调用 Claude API 进行对话
- AC-4: 实现 go-impl/internal/agent/agent.go，核心 agent 对话逻辑
- AC-5: 实现 go-impl/internal/tools/file.go，文件读写工具
- AC-6: 添加 go-impl/README.md，说明 Go 版本使用方法

---

## MUTABLE SECTION
<!-- Update each round with justification for changes -->

### Plan Version: 1 (Updated: Round 0)

#### Plan Evolution Log
<!-- Document any changes to the plan with justification -->
| Round | Change | Reason | Impact on AC |
|-------|--------|--------|--------------|
| 0 | Initial plan — populated goal tracker | - | - |

#### Active Tasks
<!-- Map each task to its target Acceptance Criterion and routing tag -->
| Task | Target AC | Status | Tag | Owner | Notes |
|------|-----------|--------|-----|-------|-------|
| task1: Create go.mod (module github.com/pp5ee/forgecode-go), go.sum | AC-1 | pending | coding | claude | Phase 1 |
| task2: Add cobra dependency (github.com/spf13/cobra) | AC-1 | pending | coding | claude | Phase 1 |
| task3: Implement go-impl/internal/api/claude.go — Claude API HTTP client | AC-3 | pending | coding | claude | Phase 2 |
| task4: Implement go-impl/internal/agent/agent.go — dialog agent logic | AC-4 | pending | coding | claude | Phase 2 |
| task5: Implement go-impl/internal/tools/file.go — file read/write tools | AC-5 | pending | coding | claude | Phase 2 |
| task6: Implement go-impl/cmd/forge/main.go — CLI entry (cobra root command) | AC-2 | pending | coding | claude | Phase 2 |
| task7: Write go-impl/README.md — build and usage instructions | AC-6 | pending | coding | claude | Phase 3 |

### Completed and Verified
<!-- Only move tasks here after Codex verification -->
| AC | Task | Completed Round | Verified Round | Evidence |
|----|------|-----------------|----------------|----------|

### Explicitly Deferred
<!-- Items here require strong justification -->
| Task | Original AC | Deferred Since | Justification | When to Reconsider |
|------|-------------|----------------|---------------|-------------------|

### Open Issues
<!-- Issues discovered during implementation -->
| Issue | Discovered Round | Blocking AC | Resolution Path |
|-------|-----------------|-------------|-----------------|
