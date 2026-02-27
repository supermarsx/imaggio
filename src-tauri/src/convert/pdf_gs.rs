use std::path::Path;
use std::process::Command;

use super::suffix_path;

fn find_gs() -> &'static str {
    // macOS: gs (from brew install ghostscript)
    // Linux: gs
    // Windows: gswin64c or gswin32c
    if cfg!(target_os = "windows") {
        "gswin64c"
    } else {
        "gs"
    }
}

pub fn compress(input: &Path, quality: &str) -> Result<Vec<String>, String> {
    let suffix = match quality {
        "screen" => "_compressed_low",
        "ebook" => "_compressed_med",
        "printer" => "_compressed_high",
        _ => "_compressed",
    };
    let output = suffix_path(input, suffix, None);

    let cmd_result = Command::new(find_gs())
        .args([
            "-sDEVICE=pdfwrite",
            "-dCompatibilityLevel=1.4",
            &format!("-dPDFSETTINGS=/{quality}"),
            "-dNOPAUSE",
            "-dQUIET",
            "-dBATCH",
            &format!("-sOutputFile={}", output.to_str().unwrap()),
            input.to_str().unwrap(),
        ])
        .output();

    match cmd_result {
        Ok(out) if out.status.success() => Ok(vec![output.to_string_lossy().to_string()]),
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            Err(format!("Ghostscript compression failed: {stderr}"))
        }
        Err(e) => Err(format!(
            "Failed to run Ghostscript (is it installed? brew install ghostscript): {e}"
        )),
    }
}

pub fn to_pdfa(input: &Path, compress_quality: Option<&str>) -> Result<Vec<String>, String> {
    let suffix = match compress_quality {
        Some("screen") => "_pdfa_low",
        Some("ebook") => "_pdfa_med",
        Some("printer") => "_pdfa_high",
        _ => "_pdfa",
    };
    let output = suffix_path(input, suffix, None);

    let mut args = vec![
        "-dPDFA=2".to_string(),
        "-dBATCH".to_string(),
        "-dNOPAUSE".to_string(),
        "-dQUIET".to_string(),
        "-sDEVICE=pdfwrite".to_string(),
        format!("-sOutputFile={}", output.to_str().unwrap()),
        "-dPDFACompatibilityPolicy=1".to_string(),
    ];

    if let Some(quality) = compress_quality {
        args.push(format!("-dPDFSETTINGS=/{quality}"));
    }

    args.push(input.to_str().unwrap().to_string());

    let cmd_result = Command::new(find_gs()).args(&args).output();

    match cmd_result {
        Ok(out) if out.status.success() => Ok(vec![output.to_string_lossy().to_string()]),
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            Err(format!("Ghostscript PDF/A conversion failed: {stderr}"))
        }
        Err(e) => Err(format!(
            "Failed to run Ghostscript (is it installed? brew install ghostscript): {e}"
        )),
    }
}

pub fn stamp(input: &Path, stamp_pdf: &Path) -> Result<Vec<String>, String> {
    let output = suffix_path(input, "_stamped", None);

    let cmd_result = Command::new(find_gs())
        .args([
            "-dBATCH",
            "-dNOPAUSE",
            "-dQUIET",
            "-sDEVICE=pdfwrite",
            &format!("-sOutputFile={}", output.to_str().unwrap()),
            stamp_pdf.to_str().unwrap(),
            input.to_str().unwrap(),
        ])
        .output();

    match cmd_result {
        Ok(out) if out.status.success() => Ok(vec![output.to_string_lossy().to_string()]),
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            Err(format!("Ghostscript stamping failed: {stderr}"))
        }
        Err(e) => Err(format!(
            "Failed to run Ghostscript (is it installed? brew install ghostscript): {e}"
        )),
    }
}

pub fn doc_to_pdf(input: &Path) -> Result<Vec<String>, String> {
    let output_dir = input.parent().unwrap_or_else(|| Path::new("."));

    // Try common LibreOffice paths
    let soffice = if cfg!(target_os = "macos") {
        "/Applications/LibreOffice.app/Contents/MacOS/soffice"
    } else {
        "soffice"
    };

    let cmd_result = Command::new(soffice)
        .args([
            "--headless",
            "--convert-to",
            "pdf",
            "--outdir",
            output_dir.to_str().unwrap(),
            input.to_str().unwrap(),
        ])
        .output();

    match cmd_result {
        Ok(out) if out.status.success() => {
            let output = suffix_path(input, "", Some("pdf"));
            if output.exists() {
                Ok(vec![output.to_string_lossy().to_string()])
            } else {
                // LibreOffice might produce the file with original stem
                let stderr = String::from_utf8_lossy(&out.stdout);
                Err(format!(
                    "LibreOffice completed but output not found. Output: {stderr}"
                ))
            }
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            Err(format!("LibreOffice conversion failed: {stderr}"))
        }
        Err(e) => Err(format!(
            "Failed to run LibreOffice (is it installed?): {e}"
        )),
    }
}
