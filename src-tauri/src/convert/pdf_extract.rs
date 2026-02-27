//! Native PDF image and attachment extraction using lopdf.

use lopdf::{Document, Object};
use std::path::Path;

pub fn extract_images(input: &Path) -> Result<Vec<String>, String> {
    let doc = Document::load(input).map_err(|e| format!("Failed to load PDF: {e}"))?;
    let output_dir = input.parent().unwrap_or_else(|| Path::new("."));
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");

    let mut outputs = Vec::new();
    let pages = doc.get_pages();
    let mut sorted_pages: Vec<_> = pages.iter().collect();
    sorted_pages.sort_by_key(|(&num, _)| num);

    for (&page_num, &page_id) in &sorted_pages {
        let page_dict = match doc.get_object(page_id).and_then(|o| o.as_dict()) {
            Ok(d) => d,
            Err(_) => continue,
        };

        // Get Resources -> XObject dictionary
        let xobjects = match get_xobjects(page_dict, &doc) {
            Some(x) => x,
            None => continue,
        };

        let mut img_idx = 0;
        for (_name, obj_ref) in xobjects.iter() {
            let obj = match obj_ref {
                Object::Reference(id) => match doc.get_object(*id) {
                    Ok(o) => o,
                    Err(_) => continue,
                },
                other => other,
            };

            if let Object::Stream(stream) = obj {
                let subtype = stream
                    .dict
                    .get(b"Subtype")
                    .ok()
                    .and_then(|o| o.as_name().ok())
                    .map(|n| n.to_vec());

                if subtype.as_deref() != Some(b"Image") {
                    continue;
                }

                img_idx += 1;

                let filter = stream
                    .dict
                    .get(b"Filter")
                    .ok()
                    .and_then(|o| match o {
                        Object::Name(n) => Some(n.clone()),
                        Object::Array(arr) => {
                            // Get the last filter (the outermost encoding)
                            arr.last().and_then(|o| o.as_name().ok()).map(|n| n.to_vec())
                        }
                        _ => None,
                    });

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
                let _bpc = stream
                    .dict
                    .get(b"BitsPerComponent")
                    .ok()
                    .and_then(|o| o.as_i64().ok())
                    .unwrap_or(8) as u32;

                match filter.as_deref() {
                    Some(b"DCTDecode") => {
                        // Raw JPEG data — save directly
                        let out_path =
                            output_dir.join(format!("{stem}_img-{page_num}-{img_idx}.jpg"));
                        std::fs::write(&out_path, &stream.content)
                            .map_err(|e| format!("Failed to write JPEG: {e}"))?;
                        outputs.push(out_path.to_string_lossy().to_string());
                    }
                    Some(b"FlateDecode") | None => {
                        // Decompress and reconstruct as PNG
                        let raw_data = if filter.as_deref() == Some(b"FlateDecode") {
                            stream
                                .decompressed_content()
                                .map_err(|e| format!("Failed to decompress: {e}"))?
                        } else {
                            stream.content.clone()
                        };

                        if width == 0 || height == 0 {
                            continue;
                        }

                        let color_space = stream
                            .dict
                            .get(b"ColorSpace")
                            .ok()
                            .and_then(|o| o.as_name().ok())
                            .map(|n| n.to_vec());

                        let out_path =
                            output_dir.join(format!("{stem}_img-{page_num}-{img_idx}.png"));

                        match color_space.as_deref() {
                            Some(b"DeviceGray") | Some(b"CalGray") => {
                                if let Some(img) =
                                    image::GrayImage::from_raw(width, height, raw_data)
                                {
                                    img.save(&out_path)
                                        .map_err(|e| format!("Failed to save PNG: {e}"))?;
                                    outputs.push(out_path.to_string_lossy().to_string());
                                }
                            }
                            _ => {
                                // Assume DeviceRGB
                                if let Some(img) =
                                    image::RgbImage::from_raw(width, height, raw_data)
                                {
                                    img.save(&out_path)
                                        .map_err(|e| format!("Failed to save PNG: {e}"))?;
                                    outputs.push(out_path.to_string_lossy().to_string());
                                }
                            }
                        }
                    }
                    Some(b"JPXDecode") => {
                        // JPEG2000 — save raw bytes
                        let out_path =
                            output_dir.join(format!("{stem}_img-{page_num}-{img_idx}.jp2"));
                        std::fs::write(&out_path, &stream.content)
                            .map_err(|e| format!("Failed to write JP2: {e}"))?;
                        outputs.push(out_path.to_string_lossy().to_string());
                    }
                    _ => {
                        // Unsupported filter — skip
                        continue;
                    }
                }
            }
        }
    }

    if outputs.is_empty() {
        Err("No images found in PDF".to_string())
    } else {
        Ok(outputs)
    }
}

fn get_xobjects<'a>(
    page_dict: &'a lopdf::Dictionary,
    doc: &'a Document,
) -> Option<&'a lopdf::Dictionary> {
    let resources = page_dict.get(b"Resources").ok()?;
    let resources = match resources {
        Object::Reference(id) => doc.get_object(*id).ok()?,
        other => other,
    };
    let resources = resources.as_dict().ok()?;

    let xobjects = resources.get(b"XObject").ok()?;
    let xobjects = match xobjects {
        Object::Reference(id) => doc.get_object(*id).ok()?,
        other => other,
    };
    xobjects.as_dict().ok()
}

pub fn detach_files(input: &Path) -> Result<Vec<String>, String> {
    let doc = Document::load(input).map_err(|e| format!("Failed to load PDF: {e}"))?;
    let output_dir = input.parent().unwrap_or_else(|| Path::new("."));

    let mut outputs = Vec::new();

    // Navigate: catalog -> Names -> EmbeddedFiles
    let catalog = doc.catalog().map_err(|e| format!("Failed to get catalog: {e}"))?;

    let names = match catalog.get(b"Names") {
        Ok(obj) => resolve_obj(obj, &doc),
        Err(_) => return Err("No embedded files found in PDF".to_string()),
    };

    let names_dict = names
        .as_dict()
        .map_err(|_| "Names is not a dictionary".to_string())?;

    let embedded = match names_dict.get(b"EmbeddedFiles") {
        Ok(obj) => resolve_obj(obj, &doc),
        Err(_) => return Err("No embedded files found in PDF".to_string()),
    };

    // Walk the name tree
    let entries = collect_name_tree_entries(embedded, &doc);

    for (filename, filespec_obj) in entries {
        let filespec = match resolve_obj(filespec_obj, &doc).as_dict() {
            Ok(d) => d.clone(),
            Err(_) => continue,
        };

        // Get the embedded file stream from /EF -> /F
        let ef = match filespec.get(b"EF") {
            Ok(obj) => match resolve_obj(obj, &doc).as_dict() {
                Ok(d) => d.clone(),
                Err(_) => continue,
            },
            Err(_) => continue,
        };

        let stream_ref = match ef.get(b"F") {
            Ok(obj) => obj.clone(),
            Err(_) => continue,
        };

        let stream_obj = match &stream_ref {
            Object::Reference(id) => match doc.get_object(*id) {
                Ok(o) => o,
                Err(_) => continue,
            },
            other => other,
        };

        if let Object::Stream(stream) = stream_obj {
            let data = stream
                .decompressed_content()
                .unwrap_or_else(|_| stream.content.clone());

            // Use the filename from the name tree, or /UF, or /F from filespec
            let out_name = if !filename.is_empty() {
                filename.clone()
            } else {
                filespec
                    .get(b"UF")
                    .or_else(|_| filespec.get(b"F"))
                    .ok()
                    .and_then(|o| {
                        if let Object::String(bytes, _) = o {
                            Some(String::from_utf8_lossy(bytes).to_string())
                        } else {
                            None
                        }
                    })
                    .unwrap_or_else(|| format!("attachment_{}", outputs.len()))
            };

            let out_path = output_dir.join(&out_name);
            std::fs::write(&out_path, &data)
                .map_err(|e| format!("Failed to write attachment: {e}"))?;
            outputs.push(out_path.to_string_lossy().to_string());
        }
    }

    if outputs.is_empty() {
        Err("No embedded files found in PDF".to_string())
    } else {
        Ok(outputs)
    }
}

fn resolve_obj<'a>(obj: &'a Object, doc: &'a Document) -> &'a Object {
    match obj {
        Object::Reference(id) => doc.get_object(*id).unwrap_or(obj),
        _ => obj,
    }
}

fn collect_name_tree_entries<'a>(
    node: &'a Object,
    doc: &'a Document,
) -> Vec<(String, &'a Object)> {
    let mut entries = Vec::new();

    let dict = match resolve_obj(node, doc).as_dict() {
        Ok(d) => d,
        Err(_) => return entries,
    };

    // Leaf node: has /Names array
    if let Ok(names_arr) = dict.get(b"Names") {
        if let Ok(arr) = resolve_obj(names_arr, doc).as_array() {
            // Array is [name1, value1, name2, value2, ...]
            let mut i = 0;
            while i + 1 < arr.len() {
                let name = match &arr[i] {
                    Object::String(bytes, _) => String::from_utf8_lossy(bytes).to_string(),
                    _ => String::new(),
                };
                entries.push((name, &arr[i + 1]));
                i += 2;
            }
        }
    }

    // Intermediate node: has /Kids array
    if let Ok(kids) = dict.get(b"Kids") {
        if let Ok(arr) = resolve_obj(kids, doc).as_array() {
            for kid in arr {
                entries.extend(collect_name_tree_entries(kid, doc));
            }
        }
    }

    entries
}
