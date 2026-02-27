use std::path::Path;

use super::suffix_path;

pub fn extract_text(input: &Path) -> Result<Vec<String>, String> {
    let bytes = std::fs::read(input).map_err(|e| e.to_string())?;
    let text = pdf_extract::extract_text_from_mem(&bytes).map_err(|e| e.to_string())?;
    let output = suffix_path(input, "", Some("txt"));
    std::fs::write(&output, text).map_err(|e| e.to_string())?;
    Ok(vec![output.to_string_lossy().to_string()])
}
