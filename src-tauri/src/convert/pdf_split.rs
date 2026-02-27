use lopdf::{Document, Object, ObjectId};
use std::collections::BTreeMap;
use std::path::Path;

use super::suffix_path;

pub fn split_pages(input: &Path) -> Result<Vec<String>, String> {
    let doc = Document::load(input).map_err(|e| e.to_string())?;
    let pages = doc.get_pages();
    let total = pages.len();

    if total == 0 {
        return Err("PDF has no pages".to_string());
    }

    let mut outputs = Vec::new();

    for page_num in 1..=total as u32 {
        let mut new_doc = doc.clone();

        // Collect page IDs to remove (all except current page)
        let pages_to_remove: Vec<u32> = new_doc
            .get_pages()
            .keys()
            .filter(|&&num| num != page_num)
            .copied()
            .collect();

        new_doc.delete_pages(&pages_to_remove);

        let suffix = format!("_page{page_num}");
        let output = suffix_path(input, &suffix, None);
        new_doc.save(&output).map_err(|e| e.to_string())?;
        outputs.push(output.to_string_lossy().to_string());
    }

    Ok(outputs)
}

pub fn join_pdfs(folder: &Path) -> Result<Vec<String>, String> {
    if !folder.is_dir() {
        return Err("Input must be a folder for PDF join".to_string());
    }

    let mut pdf_files: Vec<_> = std::fs::read_dir(folder)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("pdf"))
                .unwrap_or(false)
        })
        .collect();

    pdf_files.sort();

    if pdf_files.is_empty() {
        return Err("No PDF files found in folder".to_string());
    }

    // Start with the first document
    let mut merged = Document::load(&pdf_files[0]).map_err(|e| e.to_string())?;

    for pdf_path in &pdf_files[1..] {
        let doc = Document::load(pdf_path).map_err(|e| e.to_string())?;

        // Remap and insert all objects from the source document
        let mut id_map: BTreeMap<ObjectId, ObjectId> = BTreeMap::new();

        for (old_id, object) in doc.objects.iter() {
            let new_id = merged.add_object(object.clone());
            id_map.insert(*old_id, new_id);
        }

        // Get the source page references and add them to the merged doc's page tree
        let source_pages = doc.get_pages();
        let mut page_ids: Vec<_> = source_pages.iter().collect();
        page_ids.sort_by_key(|(&num, _)| num);

        for (_, &old_page_id) in page_ids {
            if let Some(&new_page_id) = id_map.get(&old_page_id) {
                // Add page reference to merged document's page tree
                if let Ok(catalog) = merged.catalog_mut() {
                    if let Ok(pages_ref) = catalog.get(b"Pages") {
                        if let Ok(pages_id) = pages_ref.clone().as_reference() {
                            if let Ok(pages_dict) = merged.get_object_mut(pages_id) {
                                if let Object::Dictionary(ref mut dict) = pages_dict {
                                    if let Ok(kids) = dict.get_mut(b"Kids") {
                                        if let Object::Array(ref mut arr) = kids {
                                            arr.push(Object::Reference(new_page_id));
                                        }
                                    }
                                    // Update count
                                    if let Ok(count) = dict.get(b"Count") {
                                        if let Ok(n) = count.as_i64() {
                                            dict.set(
                                                b"Count".to_vec(),
                                                Object::Integer(n + 1),
                                            );
                                        }
                                    }
                                    // Set parent on the new page
                                    if let Ok(page_obj) = merged.get_object_mut(new_page_id) {
                                        if let Object::Dictionary(ref mut page_dict) = page_obj {
                                            page_dict.set(
                                                b"Parent".to_vec(),
                                                Object::Reference(pages_id),
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let output = folder.join("joined.pdf");
    merged.save(&output).map_err(|e| e.to_string())?;
    Ok(vec![output.to_string_lossy().to_string()])
}
