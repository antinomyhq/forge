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
