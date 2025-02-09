# Code-Forge ⚡

[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg?style=for-the-badge)](https://opensource.org/licenses/Apache-2.0)
[![Rust](https://img.shields.io/badge/Rust-1.70%2B-orange.svg?style=for-the-badge)](https://www.rust-lang.org)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg?style=for-the-badge)](CONTRIBUTING.md)
[![CI Status](https://img.shields.io/github/actions/workflow/status/antinomyhq/forge/ci.yml?style=for-the-badge)](https://github.com/antinomyhq/forge/actions)
[![GitHub Release](https://img.shields.io/github/v/release/antinomyhq/forge?style=for-the-badge)](https://github.com/antinomyhq/forge/releases)
[![Last Commit](https://img.shields.io/github/last-commit/antinomyhq/forge?style=for-the-badge)](https://github.com/antinomyhq/forge/commits)
[![Open Issues](https://img.shields.io/github/issues/antinomyhq/forge?style=for-the-badge)](https://github.com/antinomyhq/forge/issues)
[![Open PRs](https://img.shields.io/github/issues-pr/antinomyhq/forge?style=for-the-badge)](https://github.com/antinomyhq/forge/pulls)
[![Repo Size](https://img.shields.io/github/repo-size/antinomyhq/forge?style=for-the-badge)](https://github.com/antinomyhq/forge)
[![Discord](https://img.shields.io/badge/Discord-Join%20Us-blue?style=for-the-badge)](https://discord.gg/Rdyu7hgSWm)

An open-source AI powered interactive shell

## Installation

**Mac**

```
brew tap antinomyhq/code-forge
brew install code-forge
```

**Linux**

```bash
# Download and install in one command
curl -L https://raw.githubusercontent.com/antinomyhq/forge/main/install.sh | bash

# Or with wget
wget -qO- https://raw.githubusercontent.com/antinomyhq/forge/main/install.sh | bash
```

## Get Started

1. Create a `.env` file in your home directory and set the following variables:

   ```bash
   OPEN_ROUTER_KEY=[Enter your Open Router Key]
   FORGE_LARGE_MODEL=anthropic/claude-3.5-sonnet
   FORGE_SMALL_MODEL=anthropic/claude-3.5-haiku

   ```

2. Start an interactive shell by typing `forge`:

   ```bash
   forge
   ⚡ # Write your task here and press enter or type
   ```

3. Use `@` and press the `tab` key to enable auto-completion of files.

4. Use `/` and press the `tab` key to access built in commands

Use `forge --help` to configure additional parameters.
