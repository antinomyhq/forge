use std::path::{Path, PathBuf};

use crate::install::InstallationStrategy;

#[derive(Debug, Clone)]
pub struct ServerDefinition {
    pub id: String,
    pub language_ids: Vec<String>,
    pub root_markers: Vec<String>,
    pub command: String,
    pub args: Vec<String>,
    pub installation: Option<InstallationStrategy>,
}

impl ServerDefinition {
    pub fn new(
        id: &str,
        language_ids: &[&str],
        root_markers: &[&str],
        command: &str,
        args: &[&str],
        installation: Option<InstallationStrategy>,
    ) -> Self {
        Self {
            id: id.to_string(),
            language_ids: language_ids.iter().map(|s| s.to_string()).collect(),
            root_markers: root_markers.iter().map(|s| s.to_string()).collect(),
            command: command.to_string(),
            args: args.iter().map(|s| s.to_string()).collect(),
            installation,
        }
    }
}

pub fn get_server_definitions() -> Vec<ServerDefinition> {
    vec![
        ServerDefinition::new(
            "rust-analyzer",
            &["rust"],
            &["Cargo.toml"],
            "rust-analyzer",
            &[],
            None,
        ),
        ServerDefinition::new(
            "typescript-language-server",
            &[
                "typescript",
                "typescriptreact",
                "javascript",
                "javascriptreact",
            ],
            &["package.json", "tsconfig.json", "jsconfig.json"],
            "typescript-language-server",
            &["--stdio"],
            Some(InstallationStrategy::Npm {
                package: "typescript-language-server typescript".to_string(),
            }),
        ),
        ServerDefinition::new(
            "gopls",
            &["go"],
            &["go.mod"],
            "gopls",
            &[],
            Some(InstallationStrategy::Go {
                package: "golang.org/x/tools/gopls@latest".to_string(),
            }),
        ),
        ServerDefinition::new(
            "pyright",
            &["python"],
            &["pyproject.toml", "setup.py", "requirements.txt"],
            "pyright-langserver",
            &["--stdio"],
            Some(InstallationStrategy::Npm { package: "pyright".to_string() }),
        ),
        // New languages
        ServerDefinition::new(
            "csharp-ls",
            &["csharp"],
            &["*.sln", "*.csproj"],
            "csharp-ls",
            &[],
            Some(InstallationStrategy::Dotnet { package: "csharp-ls".to_string() }),
        ),
        ServerDefinition::new(
            "vue-language-server",
            &["vue"],
            &["package.json"],
            "vue-language-server",
            &["--stdio"],
            Some(InstallationStrategy::Npm {
                package: "@vue/language-server typescript@latest".to_string(),
            }),
        ),
        ServerDefinition::new(
            "svelte-language-server",
            &["svelte"],
            &["package.json", "svelte.config.js"],
            "svelteserver",
            &["--stdio"],
            Some(InstallationStrategy::Npm { package: "svelte-language-server".to_string() }),
        ),
        ServerDefinition::new(
            "bash-language-server",
            &["shellscript"],
            &[".git"], // Fallback to git root usually
            "bash-language-server",
            &["start"],
            Some(InstallationStrategy::Npm { package: "bash-language-server".to_string() }),
        ),
        ServerDefinition::new(
            "yaml-language-server",
            &["yaml"],
            &[".git"],
            "yaml-language-server",
            &["--stdio"],
            Some(InstallationStrategy::Npm { package: "yaml-language-server".to_string() }),
        ),
        ServerDefinition::new(
            "dockerfile-language-server",
            &["dockerfile"],
            &["Dockerfile"],
            "docker-langserver",
            &["--stdio"],
            Some(InstallationStrategy::Npm {
                package: "dockerfile-language-server-nodejs".to_string(),
            }),
        ),
        ServerDefinition::new(
            "vscode-html-language-server",
            &["html"],
            &["package.json", ".git"],
            "vscode-html-language-server",
            &["--stdio"],
            Some(InstallationStrategy::Npm { package: "vscode-langservers-extracted".to_string() }),
        ),
        ServerDefinition::new(
            "vscode-css-language-server",
            &["css", "scss", "less"],
            &["package.json", ".git"],
            "vscode-css-language-server",
            &["--stdio"],
            Some(InstallationStrategy::Npm { package: "vscode-langservers-extracted".to_string() }),
        ),
        ServerDefinition::new(
            "vscode-json-language-server",
            &["json", "jsonc"],
            &["package.json", ".git"],
            "vscode-json-language-server",
            &["--stdio"],
            Some(InstallationStrategy::Npm { package: "vscode-langservers-extracted".to_string() }),
        ),
    ]
}

pub fn find_root(path: &Path, markers: &[String]) -> Option<PathBuf> {
    let mut current = if path.is_file() { path.parent()? } else { path };

    loop {
        for marker in markers {
            // Handle wildcard markers like *.sln
            if marker.starts_with("*.") {
                let ext = &marker[1..]; // .sln
                if let Ok(entries) = std::fs::read_dir(current) {
                    for entry in entries.flatten() {
                        if let Some(name) = entry.file_name().to_str() {
                            if name.ends_with(ext) {
                                return Some(current.to_path_buf());
                            }
                        }
                    }
                }
            } else if current.join(marker).exists() {
                return Some(current.to_path_buf());
            }
        }

        // Also check for .git directory as a fallback for project root
        if current.join(".git").exists() {
            return Some(current.to_path_buf());
        }

        if let Some(parent) = current.parent() {
            current = parent;
        } else {
            break;
        }
    }
    None
}
