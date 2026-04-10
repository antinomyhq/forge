use chrono::Utc;
use std::path::PathBuf;
use url::Url;

fn get_images_dir() -> Option<PathBuf> {
    let base = dirs::data_local_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join(".local/share")))
        .unwrap_or_else(std::env::temp_dir);
    let images_dir = base.join("forge").join("images");
    std::fs::create_dir_all(&images_dir).ok()?;
    Some(images_dir)
}

pub enum ClipboardContent {
    Images(Vec<PathBuf>),
    Text(String),
    None,
}

fn get_image_extension(data: &[u8]) -> Option<&'static str> {
    if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("png")
    } else if data.starts_with(b"\xff\xd8\xff") {
        Some("jpg")
    } else if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        Some("gif")
    } else if data.starts_with(b"RIFF") && data.len() >= 12 && &data[8..12] == b"WEBP" {
        Some("webp")
    } else if data.starts_with(b"BM") {
        Some("bmp")
    } else {
        None
    }
}

fn has_image_mimetype() -> bool {
    if let Ok(output) = std::process::Command::new("wl-paste")
        .arg("--list-types")
        .output()
    {
        if output.status.success() {
            return String::from_utf8_lossy(&output.stdout).contains("image/");
        }
    }
    if let Ok(output) = std::process::Command::new("xclip")
        .args(&["-selection", "clipboard", "-t", "TARGETS", "-o"])
        .output()
    {
        if output.status.success() {
            return String::from_utf8_lossy(&output.stdout).contains("image/");
        }
    }
    // If tools fail (e.g. on macOS/Windows), we assume true to allow arboard to try
    true
}

fn strip_quotes(line: &str) -> &str {
    if line.len() >= 2
        && ((line.starts_with('"') && line.ends_with('"'))
            || (line.starts_with('\'') && line.ends_with('\'')))
    {
        &line[1..line.len() - 1]
    } else {
        line
    }
}

fn extract_image_paths(text: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for line in text.lines() {
        let line = strip_quotes(line.trim());

        if line.starts_with("file://") {
            if let Ok(url) = Url::parse(line) {
                if let Ok(p) = url.to_file_path() {
                    paths.push(p);
                }
            }
        } else if line.starts_with('/') {
            let p = PathBuf::from(line);
            if p.exists() && p.is_file() {
                paths.push(p);
            }
        }
    }

    let mut image_paths = Vec::new();
    for p in paths {
        if let Some(ext) = p.extension() {
            let ext_str = ext.to_string_lossy().to_lowercase();
            if matches!(
                ext_str.as_str(),
                "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp"
            ) {
                image_paths.push(p);
            }
        }
    }
    image_paths
}

pub fn paste_clipboard() -> ClipboardContent {
    let images_dir = match get_images_dir() {
        Some(d) => d,
        None => {
            tracing::error!("Failed to create images directory.");
            return ClipboardContent::None;
        }
    };

    let has_img = has_image_mimetype();

    if has_img {
        let image_types = [
            "image/png",
            "image/jpeg",
            "image/webp",
            "image/gif",
            "image/bmp",
        ];

        for img_type in image_types {
            if let Ok(output) = std::process::Command::new("wl-paste")
                .args(&["-t", img_type])
                .output()
            {
                if output.status.success() && !output.stdout.is_empty() {
                    if let Some(ext) = get_image_extension(&output.stdout) {
                        let filename =
                            format!("forge_paste_{}.{}", Utc::now().timestamp_millis(), ext);
                        let path = images_dir.join(&filename);
                        if std::fs::write(&path, &output.stdout).is_ok() {
                            tracing::info!("Successfully pasted image from clipboard (wl-paste)");
                            return ClipboardContent::Images(vec![path]);
                        }
                    }
                }
            }
        }

        for img_type in image_types {
            if let Ok(output) = std::process::Command::new("xclip")
                .args(&["-selection", "clipboard", "-t", img_type, "-o"])
                .output()
            {
                if output.status.success() && !output.stdout.is_empty() {
                    if let Some(ext) = get_image_extension(&output.stdout) {
                        let filename =
                            format!("forge_paste_{}.{}", Utc::now().timestamp_millis(), ext);
                        let path = images_dir.join(&filename);
                        if std::fs::write(&path, &output.stdout).is_ok() {
                            tracing::info!("Successfully pasted image from clipboard (xclip)");
                            return ClipboardContent::Images(vec![path]);
                        }
                    }
                }
            }
        }
    }

    if let Ok(mut clipboard) = arboard::Clipboard::new() {
        if has_img {
            if let Ok(image_data) = clipboard.get_image() {
                let width = image_data.width as u32;
                let height = image_data.height as u32;
                if let Some(img) = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(
                    width,
                    height,
                    image_data.bytes.into_owned(),
                ) {
                    let filename = format!("forge_paste_{}.png", Utc::now().timestamp_millis());
                    let path = images_dir.join(&filename);
                    if img.save(&path).is_ok() {
                        tracing::info!(
                            "Successfully pasted image ({}x{}) from clipboard",
                            width, height
                        );
                        return ClipboardContent::Images(vec![path]);
                    }
                }
            }
        }

        if let Ok(text) = clipboard.get_text() {
            let image_paths = extract_image_paths(&text);
            if !image_paths.is_empty() {
                tracing::info!(
                    "Successfully pasted {} image file path(s) from clipboard",
                    image_paths.len()
                );
                return ClipboardContent::Images(image_paths);
            }
            return ClipboardContent::Text(text);
        } else {
            tracing::info!("Clipboard does not contain an image or valid image paths.");
        }
    } else {
        if let Ok(output) = std::process::Command::new("wl-paste").output() {
            if output.status.success() && !output.stdout.is_empty() {
                if let Ok(text) = String::from_utf8(output.stdout) {
                    return ClipboardContent::Text(text);
                }
            }
        }
        if let Ok(output) = std::process::Command::new("xclip")
            .args(&["-selection", "clipboard", "-o"])
            .output()
        {
            if output.status.success() && !output.stdout.is_empty() {
                if let Ok(text) = String::from_utf8(output.stdout) {
                    return ClipboardContent::Text(text);
                }
            }
        }
        tracing::error!("Could not connect to system clipboard.");
    }

    ClipboardContent::None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_image_extension() {
        assert_eq!(get_image_extension(b"\x89PNG\r\n\x1a\n..."), Some("png"));
        assert_eq!(get_image_extension(b"\xff\xd8\xff\xdb..."), Some("jpg"));
        assert_eq!(get_image_extension(b"GIF87a..."), Some("gif"));
        assert_eq!(get_image_extension(b"GIF89a..."), Some("gif"));
        assert_eq!(
            get_image_extension(b"RIFF\x00\x00\x00\x00WEBP..."),
            Some("webp")
        );
        assert_eq!(get_image_extension(b"BM..."), Some("bmp"));
        assert_eq!(get_image_extension(b"unknown data"), None);
        assert_eq!(get_image_extension(b""), None);
    }

    #[test]
    fn test_strip_quotes() {
        assert_eq!(strip_quotes(""), "");
        assert_eq!(strip_quotes("\""), "\"");
        assert_eq!(strip_quotes("'"), "'");
        assert_eq!(strip_quotes("\"a\""), "a");
        assert_eq!(strip_quotes("'b'"), "b");
        assert_eq!(strip_quotes("\"abc\""), "abc");
        assert_eq!(strip_quotes("'xyz'"), "xyz");
        assert_eq!(strip_quotes("\"unterminated"), "\"unterminated");
        assert_eq!(strip_quotes("unquoted"), "unquoted");
    }

    #[test]
    fn test_extract_image_paths() {
        let temp_dir = std::env::temp_dir();
        let test_png = temp_dir.join("test_image.png");
        std::fs::write(&test_png, b"").unwrap();

        let test_txt = temp_dir.join("test_file.txt");
        std::fs::write(&test_txt, b"").unwrap();

        let input1 = format!("\"{}\"", test_png.display());
        let paths1 = extract_image_paths(&input1);
        assert_eq!(paths1.len(), 1);
        assert_eq!(paths1[0], test_png);

        let input2 = format!("file://{}", test_png.display());
        let paths2 = extract_image_paths(&input2);
        assert_eq!(paths2.len(), 1);
        assert_eq!(paths2[0], test_png);

        let input3 = format!("\"{}\"", test_txt.display());
        let paths3 = extract_image_paths(&input3);
        assert_eq!(paths3.len(), 0);

        let _ = std::fs::remove_file(test_png);
        let _ = std::fs::remove_file(test_txt);
    }
}
