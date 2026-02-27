use std::path::Path;
use std::process::Command;

pub fn extract_images(input: &Path) -> Result<Vec<String>, String> {
    let output_dir = input.parent().unwrap_or_else(|| Path::new("."));
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");

    let output_prefix = output_dir.join(format!("{stem}_img"));

    let cmd_result = Command::new("pdfimages")
        .args([
            "-png",
            input.to_str().unwrap(),
            output_prefix.to_str().unwrap(),
        ])
        .output();

    match cmd_result {
        Ok(out) if out.status.success() => {
            let prefix = format!("{stem}_img");
            let mut files = Vec::new();
            if let Ok(entries) = std::fs::read_dir(output_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        if name.starts_with(&prefix) && name.ends_with(".png") {
                            files.push(path.to_string_lossy().to_string());
                        }
                    }
                }
            }
            if files.is_empty() {
                Err("No images found in PDF".to_string())
            } else {
                files.sort();
                Ok(files)
            }
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            Err(format!("pdfimages failed: {stderr}"))
        }
        Err(e) => Err(format!(
            "Failed to run pdfimages (is poppler installed? brew install poppler): {e}"
        )),
    }
}

pub fn detach_files(input: &Path) -> Result<Vec<String>, String> {
    let output_dir = input.parent().unwrap_or_else(|| Path::new("."));

    // List files before detach to identify new ones
    let before: std::collections::HashSet<_> = std::fs::read_dir(output_dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .collect();

    let cmd_result = Command::new("pdfdetach")
        .args([
            "-saveall",
            "-o",
            output_dir.to_str().unwrap(),
            input.to_str().unwrap(),
        ])
        .output();

    match cmd_result {
        Ok(out) if out.status.success() => {
            let after: std::collections::HashSet<_> = std::fs::read_dir(output_dir)
                .map_err(|e| e.to_string())?
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .collect();

            let new_files: Vec<String> = after
                .difference(&before)
                .map(|p| p.to_string_lossy().to_string())
                .collect();

            if new_files.is_empty() {
                Err("No embedded files found in PDF".to_string())
            } else {
                Ok(new_files)
            }
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            Err(format!("pdfdetach failed: {stderr}"))
        }
        Err(e) => Err(format!(
            "Failed to run pdfdetach (is poppler installed? brew install poppler): {e}"
        )),
    }
}
