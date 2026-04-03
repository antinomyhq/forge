use std::path::PathBuf;

use chrono::Utc;
use url::Url;

fn get_images_dir() -> Option<PathBuf> {
    let base_dir = dirs::data_local_dir()?;
    let images_dir = base_dir.join("forge/images");
    std::fs::create_dir_all(&images_dir).ok()?;
    Some(images_dir)
}

pub fn paste_image() -> Vec<PathBuf> {
    let images_dir = match get_images_dir() {
        Some(d) => d,
        None => {
            eprintln!("\n[Forge] Failed to create images directory.");
            return vec![];
        }
    };

    let filename = format!("forge_paste_{}.png", Utc::now().timestamp_millis());
    let path = images_dir.join(&filename);

    // 1. Try external tools for PNG dump (most reliable on Linux)
    if let Ok(output) = std::process::Command::new("wl-paste")
        .args(["-t", "image/png"])
        .output()
        && output.status.success() && !output.stdout.is_empty()
            && std::fs::write(&path, &output.stdout).is_ok() {
                eprintln!("\n[Forge] Successfully pasted image from clipboard (wl-paste)!");
                return vec![path];
            }

    if let Ok(output) = std::process::Command::new("xclip")
        .args(["-selection", "clipboard", "-t", "image/png", "-o"])
        .output()
        && output.status.success() && !output.stdout.is_empty()
            && std::fs::write(&path, &output.stdout).is_ok() {
                eprintln!("\n[Forge] Successfully pasted image from clipboard (xclip)!");
                return vec![path];
            }

    // 2. Try arboard
    if let Ok(mut clipboard) = arboard::Clipboard::new() {
        if let Ok(image_data) = clipboard.get_image() {
            let width = image_data.width as u32;
            let height = image_data.height as u32;
            if let Some(img) = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(
                width,
                height,
                image_data.bytes.into_owned(),
            )
                && img.save(&path).is_ok() {
                    eprintln!(
                        "\n[Forge] Successfully pasted image ({}x{}) from clipboard!",
                        width, height
                    );
                    return vec![path];
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
                    if let Ok(url) = Url::parse(line)
                        && let Ok(p) = url.to_file_path() {
                            paths.push(p);
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
                return image_paths;
            }

            eprintln!(
                "\n[Forge] Clipboard text does not contain valid image URIs or paths.\nText snippet: {:?}",
                &text.chars().take(100).collect::<String>()
            );
        } else {
            eprintln!("\n[Forge] Clipboard does not contain an image or valid image paths.");
        }
    } else {
        eprintln!("\n[Forge] Could not connect to system clipboard.");
    }

    vec![]
}
