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

    other => {
      return Err(format!(
        "Conversion type '{other}' not implemented in Tauri backend yet"
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
