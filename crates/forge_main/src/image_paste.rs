use chrono::Utc;
use std::path::PathBuf;
use url::Url;

fn get_images_dir() -> Option<PathBuf> {
    let home_dir = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let images_dir = PathBuf::from(home_dir).join(".local/share/forge/images");
    if std::fs::create_dir_all(&images_dir).is_ok() {
        Some(images_dir)
    } else {
        None
    }
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

pub fn paste_clipboard() -> ClipboardContent {
    let images_dir = match get_images_dir() {
        Some(d) => d,
        None => {
            eprintln!("\n[Forge] Failed to create images directory.");
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

        // 1. Try external tools for image dump (most reliable on Linux)
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
                            eprintln!(
                                "\n[Forge] Successfully pasted image from clipboard (wl-paste)!"
                            );
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
                            eprintln!(
                                "\n[Forge] Successfully pasted image from clipboard (xclip)!"
                            );
                            return ClipboardContent::Images(vec![path]);
                        }
                    }
                }
            }
        }
    }

    // 2. Try arboard
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
                        eprintln!(
                            "\n[Forge] Successfully pasted image ({}x{}) from clipboard!",
                            width, height
                        );
                        return ClipboardContent::Images(vec![path]);
                    }
                }
            }
        }

        // 3. Fallback: Check if clipboard has text that contains file URIs or paths
        if let Ok(text) = clipboard.get_text() {
            let mut paths = Vec::new();
            for line in text.lines() {
                let line = line.trim();

                // Remove wrapper quotes if present
                let line = if (line.starts_with('"') && line.ends_with('"'))
                    || (line.starts_with('\'') && line.ends_with('\''))
                {
                    &line[1..line.len() - 1]
                } else {
                    line
                };

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

            // Filter paths to ensure they look like images
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

            if !image_paths.is_empty() {
                eprintln!(
                    "\n[Forge] Successfully pasted {} image file path(s) from clipboard!",
                    image_paths.len()
                );
                return ClipboardContent::Images(image_paths);
            }

            // If no valid image paths were found, just return the text!
            return ClipboardContent::Text(text);
        } else {
            eprintln!("\n[Forge] Clipboard does not contain an image or valid image paths.");
        }
    } else {
        // Try wl-paste or xclip for text if arboard fails
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
        eprintln!("\n[Forge] Could not connect to system clipboard.");
    }

    ClipboardContent::None
}
