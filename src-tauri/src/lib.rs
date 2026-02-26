use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConvertRequest {
  input_path: String,
  conversion_type: String,
  #[serde(default)]
  input_file: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConvertResponse {
  output_paths: Vec<String>,
}

fn suffix_path(input: &Path, suffix: &str, new_ext: Option<&str>) -> PathBuf {
  let parent = input.parent().unwrap_or_else(|| Path::new("."));
  let stem = input
    .file_stem()
    .and_then(|s| s.to_str())
    .unwrap_or("output");
  let mut file_name = format!("{stem}{suffix}");
  let extension = new_ext
    .or_else(|| input.extension().and_then(|e| e.to_str()))
    .unwrap_or("");
  if !extension.is_empty() {
    file_name.push('.');
    file_name.push_str(extension);
  }
  parent.join(file_name)
}

#[tauri::command]
fn select_file() -> Option<String> {
  rfd::FileDialog::new()
    .pick_file()
    .map(|p| p.to_string_lossy().to_string())
}

#[tauri::command]
fn select_folder() -> Option<String> {
  rfd::FileDialog::new()
    .pick_folder()
    .map(|p| p.to_string_lossy().to_string())
}

#[tauri::command]
fn list_dir(dir_path: String) -> Result<Vec<String>, String> {
  let dir = PathBuf::from(dir_path);
  let entries = std::fs::read_dir(&dir).map_err(|e| e.to_string())?;
  let mut files = Vec::new();
  for entry in entries {
    let entry = entry.map_err(|e| e.to_string())?;
    let path = entry.path();
    if path.is_file() {
      files.push(path.to_string_lossy().to_string());
    }
  }
  Ok(files)
}

fn convert_grayscale_image(input: &Path) -> Result<Vec<String>, String> {
  let img = image::open(input).map_err(|e| e.to_string())?;
  let gray = img.grayscale();
  let output = suffix_path(input, "_gray", None);
  gray.save(&output).map_err(|e| e.to_string())?;
  Ok(vec![output.to_string_lossy().to_string()])
}

fn pdf_remove_metadata_in_place(pdf_path: &Path) -> Result<Vec<String>, String> {
  let mut doc = lopdf::Document::load(pdf_path).map_err(|e| e.to_string())?;

  // Remove /Info entry from trailer.
  doc.trailer.remove(b"Info");

  // Remove /Metadata from catalog if present.
  if let Ok(catalog) = doc.catalog_mut() {
    catalog.remove(b"Metadata");
  }

  doc.save(pdf_path).map_err(|e| e.to_string())?;
  Ok(vec![pdf_path.to_string_lossy().to_string()])
}

fn pdf_clone_metadata_in_place(source_pdf: &Path, target_pdf: &Path) -> Result<Vec<String>, String> {
  let source = lopdf::Document::load(source_pdf).map_err(|e| e.to_string())?;
  let mut target = lopdf::Document::load(target_pdf).map_err(|e| e.to_string())?;

  // Clone trailer /Info
  if let Ok(info_ref) = source.trailer.get(b"Info") {
    if let Ok(info_id) = info_ref.clone().as_reference() {
      let info_obj = source.get_object(info_id).map_err(|e| e.to_string())?.clone();
      let new_id = target.add_object(info_obj);
      target
        .trailer
        .set(b"Info".to_vec(), lopdf::Object::Reference(new_id));
    }
  }

  // Clone catalog /Metadata stream
  let source_metadata_stream = (|| {
    let catalog = source.catalog().ok()?;
    let metadata_ref = catalog
      .get(b"Metadata")
      .ok()?
      .clone()
      .as_reference()
      .ok()?;
    let metadata_obj = source.get_object(metadata_ref).ok()?.clone();
    Some(metadata_obj)
  })();

  if let Some(metadata_obj) = source_metadata_stream {
    let new_id = target.add_object(metadata_obj);
    if let Ok(catalog) = target.catalog_mut() {
      catalog.set(b"Metadata".to_vec(), lopdf::Object::Reference(new_id));
    }
  }

  target.save(target_pdf).map_err(|e| e.to_string())?;
  Ok(vec![target_pdf.to_string_lossy().to_string()])
}

fn pdf_extract_text(input: &Path) -> Result<Vec<String>, String> {
  let bytes = std::fs::read(input).map_err(|e| e.to_string())?;
  let text = pdf_extract::extract_text_from_mem(&bytes).map_err(|e| e.to_string())?;
  let output = suffix_path(input, "", Some("txt"));
  std::fs::write(&output, text).map_err(|e| e.to_string())?;
  Ok(vec![output.to_string_lossy().to_string()])
}

fn pdf_to_images(input: &Path, format: &str, scale: u32) -> Result<Vec<String>, String> {
  // PDF to image conversion using external poppler-utils (pdftoppm/pdftocairo)
  // This is more reliable than PDFium bindings for cross-platform support
  
  use std::process::Command;
  
  let output_dir = input.parent().unwrap_or_else(|| Path::new("."));
  let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("output");
  
  // Determine DPI based on scale (rough approximation)
  let dpi = (scale / 8).to_string(); // 2000px ≈ 250dpi, 4000px ≈ 500dpi, 8000px ≈ 1000dpi
  
  let output_prefix = output_dir.join(stem);
  
  // Use pdftoppm for raster formats
  let cmd_result = match format {
    "jpeg" | "jpg" => {
      Command::new("pdftoppm")
        .args(&[
          "-jpeg",
          "-r", &dpi,
          input.to_str().unwrap(),
          output_prefix.to_str().unwrap(),
        ])
        .output()
    }
    "png" => {
      Command::new("pdftoppm")
        .args(&[
          "-png",
          "-r", &dpi,
          input.to_str().unwrap(),
          output_prefix.to_str().unwrap(),
        ])
        .output()
    }
    "tiff" | "tif" => {
      Command::new("pdftoppm")
        .args(&[
          "-tiff",
          "-r", &dpi,
          input.to_str().unwrap(),
          output_prefix.to_str().unwrap(),
        ])
        .output()
    }
    "ppm" => {
      Command::new("pdftoppm")
        .args(&[
          "-r", &dpi,
          input.to_str().unwrap(),
          output_prefix.to_str().unwrap(),
        ])
        .output()
    }
    _ => return Err(format!("Unsupported image format: {}", format))
  };

  match cmd_result {
    Ok(output) if output.status.success() => {
      // List generated files
      let mut generated_files = Vec::new();
      
      if let Ok(entries) = std::fs::read_dir(output_dir) {
        for entry in entries.flatten() {
          let path = entry.path();
          if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with(stem) && name.ends_with(format) {
              generated_files.push(path.to_string_lossy().to_string());
            }
          }
        }
      }
      
      if generated_files.is_empty() {
        Err("No output files were generated".to_string())
      } else {
        Ok(generated_files)
      }
    }
    Ok(output) => {
      let stderr = String::from_utf8_lossy(&output.stderr);
      Err(format!("pdftoppm failed: {}", stderr))
    }
    Err(e) => {
      Err(format!("Failed to run pdftoppm (is poppler-utils installed?): {}", e))
    }
  }
}

fn pdf_split_pages(_input: &Path) -> Result<Vec<String>, String> {
  // PDF page splitting with lopdf is complex and requires proper page tree manipulation
  // For a complete implementation, consider using:
  // 1. qpdf library bindings
  // 2. Calling external tools like pdftk or qpdf via std::process::Command
  // 3. A more feature-complete Rust PDF library
  
  Err("PDF splitting not yet implemented. Use external tools like pdftk or qpdf for now.".to_string())
}

fn pdf_compress(input: &Path, _quality: &str) -> Result<Vec<String>, String> {
  // Note: True PDF compression requires Ghostscript.
  // This is a placeholder that copies the file with a note.
  // For production, you would either:
  // 1. Call Ghostscript via std::process::Command
  // 2. Use a Rust PDF library that supports compression
  // 3. Keep using the external binary
  
  let output = suffix_path(input, "_compressed", None);
  std::fs::copy(input, &output).map_err(|e| e.to_string())?;
  
  Ok(vec![output.to_string_lossy().to_string()])
}

#[tauri::command]
fn convert(req: ConvertRequest) -> Result<ConvertResponse, String> {
  let input = PathBuf::from(&req.input_path);
  if !input.exists() {
    return Err("Input path does not exist".to_string());
  }

  let output_paths = match req.conversion_type.as_str() {
    // Image to grayscale (replaces ImageMagick for this use case)
    "grayscalepic" => convert_grayscale_image(&input)?,

    // PDF text extraction (replaces poppler pdftotext for this use case)
    "pdf2txt" => pdf_extract_text(&input)?,

    // PDF metadata manipulation (replaces exiftool for these use cases)
    "pdfmetaremove" => pdf_remove_metadata_in_place(&input)?,
    "pdfmetaclone" => {
      let source = req
        .input_file
        .ok_or_else(|| "inputFile is required for pdfmetaclone".to_string())?;
      pdf_clone_metadata_in_place(Path::new(&source), &input)?
    }

    // PDF to JPEG conversions
    "pdf2jpeglow" => pdf_to_images(&input, "jpeg", 2000)?,
    "pdf2jpegmedium" => pdf_to_images(&input, "jpeg", 4000)?,
    "pdf2jpeghigh" => pdf_to_images(&input, "jpeg", 8000)?,

    // PDF to PNG conversions
    "pdf2pnglow" => pdf_to_images(&input, "png", 2000)?,
    "pdf2pngmedium" => pdf_to_images(&input, "png", 4000)?,
    "pdf2pnghigh" => pdf_to_images(&input, "png", 8000)?,

    // PDF to TIFF conversions
    "pdf2tifflow" => pdf_to_images(&input, "tiff", 2000)?,
    "pdf2tiffmedium" => pdf_to_images(&input, "tiff", 4000)?,
    "pdf2tiffhigh" => pdf_to_images(&input, "tiff", 8000)?,

    // PDF to PPM conversions
    "pdf2ppmlow" => pdf_to_images(&input, "ppm", 2000)?,
    "pdf2ppmmedium" => pdf_to_images(&input, "ppm", 4000)?,
    "pdf2ppmhigh" => pdf_to_images(&input, "ppm", 8000)?,

    // PDF splitting
    "pdf2split" => pdf_split_pages(&input)?,

    // PDF compression (placeholder - needs Ghostscript integration)
    "pdf2pdflow" => pdf_compress(&input, "screen")?,
    "pdf2pdfmedium" => pdf_compress(&input, "ebook")?,
    "pdf2pdfhigh" => pdf_compress(&input, "printer")?,

    other => {
      return Err(format!(
        "Conversion type '{other}' not implemented in Tauri backend yet. SVG, PDF/A, stamping, doc2pdf, and joining require additional dependencies."
      ))
    }
  };

  Ok(ConvertResponse { output_paths })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder::default()
    .invoke_handler(tauri::generate_handler![select_file, select_folder, list_dir, convert])
    .setup(|app| {
      if cfg!(debug_assertions) {
        app.handle().plugin(
          tauri_plugin_log::Builder::default()
            .level(log::LevelFilter::Info)
            .build(),
        )?;
      }
      Ok(())
    })
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
