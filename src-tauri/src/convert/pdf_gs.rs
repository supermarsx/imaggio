//! Native PDF compression, stamping, and doc-to-PDF (LibreOffice fallback).

use lopdf::{Document, Object, ObjectId};
use std::path::Path;
use std::process::Command;

use super::suffix_path;

pub fn compress(input: &Path, quality: &str) -> Result<Vec<String>, String> {
    let mut doc = Document::load(input).map_err(|e| format!("Failed to load PDF: {e}"))?;

    let suffix = match quality {
        "screen" => "_compressed_low",
        "ebook" => "_compressed_med",
        "printer" => "_compressed_high",
        _ => "_compressed",
    };
    let output = suffix_path(input, suffix, None);

    // Structural optimization
    doc.prune_objects();
    doc.delete_zero_length_streams();
    doc.compress();

    // For lower quality tiers, attempt to downscale embedded images
    if quality == "screen" || quality == "ebook" {
        let scale = if quality == "screen" { 0.25 } else { 0.5 };
        let _ = downscale_images(&mut doc, scale);
    }

    doc.save(&output).map_err(|e| format!("Failed to save compressed PDF: {e}"))?;
    Ok(vec![output.to_string_lossy().to_string()])
}

pub fn compress_images_in_doc(doc: &mut Document, scale: f64) -> Result<(), String> {
    downscale_images(doc, scale)
}

fn downscale_images(doc: &mut Document, scale: f64) -> Result<(), String> {
    // Collect image object IDs to process
    let image_ids: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter_map(|(id, obj)| {
            if let Object::Stream(stream) = obj {
                let is_image = stream
                    .dict
                    .get(b"Subtype")
                    .ok()
                    .and_then(|o| o.as_name().ok())
                    .map(|n| n == b"Image")
                    .unwrap_or(false);

                let filter = stream
                    .dict
                    .get(b"Filter")
                    .ok()
                    .and_then(|o| o.as_name().ok())
                    .map(|n| n.to_vec());

                // Only process JPEG images (DCTDecode) for recompression
                if is_image && filter.as_deref() == Some(b"DCTDecode") {
                    return Some(*id);
                }
            }
            None
        })
        .collect();

    for id in image_ids {
        let obj = match doc.get_object(id) {
            Ok(o) => o.clone(),
            Err(_) => continue,
        };

        if let Object::Stream(stream) = obj {
            let width = stream
                .dict
                .get(b"Width")
                .ok()
                .and_then(|o| o.as_i64().ok())
                .unwrap_or(0) as u32;
            let height = stream
                .dict
                .get(b"Height")
                .ok()
                .and_then(|o| o.as_i64().ok())
                .unwrap_or(0) as u32;

            if width == 0 || height == 0 {
                continue;
            }

            // Decode the JPEG
            let img = match image::load_from_memory(&stream.content) {
                Ok(img) => img,
                Err(_) => continue,
            };

            let new_w = ((width as f64) * scale).max(1.0) as u32;
            let new_h = ((height as f64) * scale).max(1.0) as u32;
            let resized = img.resize(new_w, new_h, image::imageops::FilterType::Lanczos3);

            // Re-encode as JPEG
            let mut buf = std::io::Cursor::new(Vec::new());
            let quality = if scale <= 0.25 { 60 } else { 75 };
            let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, quality);
            if resized
                .to_rgb8()
                .write_with_encoder(encoder)
                .is_err()
            {
                continue;
            }

            let new_data = buf.into_inner();

            // Build replacement stream
            let mut new_dict = stream.dict.clone();
            new_dict.set(b"Width".to_vec(), Object::Integer(new_w as i64));
            new_dict.set(b"Height".to_vec(), Object::Integer(new_h as i64));
            new_dict.set(b"Length".to_vec(), Object::Integer(new_data.len() as i64));

            let new_stream = lopdf::Stream::new(new_dict, new_data);
            doc.objects.insert(id, Object::Stream(new_stream));
        }
    }

    Ok(())
}

pub fn stamp(input: &Path, stamp_pdf: &Path) -> Result<Vec<String>, String> {
    let mut doc = Document::load(input).map_err(|e| format!("Failed to load PDF: {e}"))?;
    let stamp_doc = Document::load(stamp_pdf).map_err(|e| format!("Failed to load stamp PDF: {e}"))?;

    let output = suffix_path(input, "_stamped", None);

    // Get the first page of the stamp document
    let stamp_pages = stamp_doc.get_pages();
    let stamp_page_id = stamp_pages
        .values()
        .next()
        .ok_or_else(|| "Stamp PDF has no pages".to_string())?;

    let stamp_page = stamp_doc
        .get_object(*stamp_page_id)
        .map_err(|e| format!("Failed to get stamp page: {e}"))?;

    let stamp_dict = stamp_page
        .as_dict()
        .map_err(|_| "Stamp page is not a dictionary".to_string())?;

    // Get stamp content stream data
    let stamp_content = get_content_data(stamp_dict, &stamp_doc)?;

    // Import stamp resources into target document
    let stamp_resources = stamp_dict
        .get(b"Resources")
        .ok()
        .and_then(|r| match r {
            Object::Reference(id) => stamp_doc.get_object(*id).ok(),
            other => Some(other),
        })
        .and_then(|o| o.as_dict().ok())
        .cloned();

    // For each page of the input document, prepend the stamp content
    let pages = doc.get_pages();
    let page_ids: Vec<ObjectId> = pages.values().copied().collect();

    for page_id in page_ids {
        // Get existing content stream
        let page = doc
            .get_object(page_id)
            .map_err(|e| format!("Failed to get page: {e}"))?
            .clone();
        let page_dict = page
            .as_dict()
            .map_err(|_| "Page is not a dictionary".to_string())?;

        let existing_content = get_content_data(page_dict, &doc).unwrap_or_default();

        // Combine: save state, draw stamp, restore state, then draw original content
        let mut combined = Vec::new();
        combined.extend_from_slice(b"q\n");
        combined.extend_from_slice(&stamp_content);
        combined.extend_from_slice(b"\nQ\n");
        combined.extend_from_slice(&existing_content);

        // Create new content stream
        let new_stream = lopdf::Stream::new(lopdf::Dictionary::new(), combined);
        let stream_id = doc.add_object(Object::Stream(new_stream));

        // Merge stamp resources if available, then update page
        let stamp_res_clone = stamp_resources.clone();
        if let Ok(page_obj) = doc.get_object_mut(page_id) {
            if let Object::Dictionary(ref mut dict) = page_obj {
                dict.set(b"Contents".to_vec(), Object::Reference(stream_id));

                if let Some(ref stamp_res) = stamp_res_clone {
                    merge_resources_inplace(dict, stamp_res);
                }
            }
        }
    }

    doc.save(&output).map_err(|e| format!("Failed to save stamped PDF: {e}"))?;
    Ok(vec![output.to_string_lossy().to_string()])
}

fn get_content_data(page_dict: &lopdf::Dictionary, doc: &Document) -> Result<Vec<u8>, String> {
    let contents = page_dict
        .get(b"Contents")
        .map_err(|_| "No content stream".to_string())?;

    match contents {
        Object::Reference(id) => {
            let obj = doc.get_object(*id).map_err(|e| e.to_string())?;
            if let Object::Stream(stream) = obj {
                stream.decompressed_content().map_err(|e| e.to_string())
            } else {
                Err("Content is not a stream".to_string())
            }
        }
        Object::Array(arr) => {
            let mut combined = Vec::new();
            for item in arr {
                let obj = match item {
                    Object::Reference(id) => doc.get_object(*id).map_err(|e| e.to_string())?,
                    other => other,
                };
                if let Object::Stream(stream) = obj {
                    let data = stream.decompressed_content().map_err(|e| e.to_string())?;
                    combined.extend_from_slice(&data);
                    combined.push(b' ');
                }
            }
            Ok(combined)
        }
        Object::Stream(stream) => stream.decompressed_content().map_err(|e| e.to_string()),
        _ => Err("Unexpected content type".to_string()),
    }
}

fn merge_resources_inplace(
    page_dict: &mut lopdf::Dictionary,
    stamp_resources: &lopdf::Dictionary,
) {
    // For simplicity, merge top-level resource dictionaries
    // (Font, XObject, ExtGState, etc.)
    let resource_keys: &[&[u8]] = &[b"Font", b"XObject", b"ExtGState", b"ColorSpace", b"Pattern"];

    let existing_resources = page_dict
        .get(b"Resources")
        .ok()
        .and_then(|o| o.as_dict().ok())
        .cloned()
        .unwrap_or_default();

    let mut merged = existing_resources;

    for key in resource_keys {
        if let Ok(stamp_sub) = stamp_resources.get(*key) {
            if let Ok(stamp_dict) = stamp_sub.as_dict() {
                let existing_sub = merged
                    .get(*key)
                    .ok()
                    .and_then(|o| o.as_dict().ok())
                    .cloned()
                    .unwrap_or_default();

                let mut combined = existing_sub;
                for (k, v) in stamp_dict.iter() {
                    if combined.get(k).is_err() {
                        combined.set(k.clone(), v.clone());
                    }
                }
                merged.set(key.to_vec(), Object::Dictionary(combined));
            }
        }
    }

    page_dict.set(b"Resources".to_vec(), Object::Dictionary(merged));
}

pub fn doc_to_pdf(input: &Path) -> Result<Vec<String>, String> {
    let output_dir = input.parent().unwrap_or_else(|| Path::new("."));

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
                let stdout = String::from_utf8_lossy(&out.stdout);
                Err(format!(
                    "LibreOffice completed but output not found. Output: {stdout}"
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
