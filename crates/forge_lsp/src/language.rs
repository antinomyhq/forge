use std::path::Path;

pub fn get_language_id(path: &Path) -> Option<&'static str> {
    if let Some(file_name) = path.file_name() {
        if file_name == "Dockerfile" {
            return Some("dockerfile");
        }
    }

    let extension = path.extension()?.to_str()?;
    let extension = format!(".{}", extension);

    match extension.as_str() {
        ".rs" => Some("rust"),
        ".ts" | ".mts" | ".cts" => Some("typescript"),
        ".tsx" | ".mtsx" | ".ctsx" => Some("typescriptreact"),
        ".js" | ".mjs" | ".cjs" => Some("javascript"),
        ".jsx" => Some("javascriptreact"),
        ".py" => Some("python"),
        ".go" => Some("go"),
        ".c" => Some("c"),
        ".cpp" | ".cxx" | ".cc" | ".c++" => Some("cpp"),
        ".h" | ".hpp" => Some("cpp"),
        ".java" => Some("java"),
        ".lua" => Some("lua"),
        ".php" => Some("php"),
        ".rb" => Some("ruby"),
        ".sh" | ".bash" | ".zsh" => Some("shellscript"),
        ".css" => Some("css"),
        ".scss" => Some("scss"),
        ".less" => Some("less"),
        ".html" | ".htm" => Some("html"),
        ".json" => Some("json"),
        ".jsonc" => Some("jsonc"),
        ".md" | ".markdown" => Some("markdown"),
        ".xml" => Some("xml"),
        ".yaml" | ".yml" => Some("yaml"),
        ".toml" => Some("toml"),
        ".zig" | ".zon" => Some("zig"),
        ".cs" => Some("csharp"),
        ".vue" => Some("vue"),
        ".svelte" => Some("svelte"),
        ".dockerfile" => Some("dockerfile"),
        _ => None,
    }
}
