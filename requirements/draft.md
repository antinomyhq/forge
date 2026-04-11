# Requirement

这是一个 Rust 实现的 AI 编程助手工具 forgecode。我希望用 Go 语言重新实现它的核心功能。主要目标：1) 将 Rust crates 中的核心模块用 Go 重写，2) 保持 CLI 接口兼容，3) 使用 Go 的 cobra 做 CLI 框架，4) 用 Go 的 net/http 做 HTTP 客户端调用 AI API，5) 保留基本的 agent、conversation 等功能。

Go 版本的入口点放在 cmd/forge/main.go，包名统一用 forgecode。不需要完整移植所有功能，先实现一个可以工作的最小核心：CLI 入口、与 Claude API 的对话功能、基本的文件读写工具调用。新代码放在 go-impl/ 目录下。

---

## Standard Deliverables (mandatory for every project)

- **README.md** — must be included at the project root with: project title & description, prerequisites, installation steps, usage examples with code snippets, configuration options, and project structure overview.
- **Git commits** — use conventional commit prefix `feat:` for all commits.
