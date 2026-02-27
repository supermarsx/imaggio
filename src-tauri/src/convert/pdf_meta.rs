use std::path::Path;

pub fn remove_metadata(pdf_path: &Path) -> Result<Vec<String>, String> {
    let mut doc = lopdf::Document::load(pdf_path).map_err(|e| e.to_string())?;

    doc.trailer.remove(b"Info");

    if let Ok(catalog) = doc.catalog_mut() {
        catalog.remove(b"Metadata");
    }

    doc.save(pdf_path).map_err(|e| e.to_string())?;
    Ok(vec![pdf_path.to_string_lossy().to_string()])
}

pub fn clone_metadata(source_pdf: &Path, target_pdf: &Path) -> Result<Vec<String>, String> {
    let source = lopdf::Document::load(source_pdf).map_err(|e| e.to_string())?;
    let mut target = lopdf::Document::load(target_pdf).map_err(|e| e.to_string())?;

    // Clone trailer /Info
    if let Ok(info_ref) = source.trailer.get(b"Info") {
        if let Ok(info_id) = info_ref.clone().as_reference() {
            let info_obj = source
                .get_object(info_id)
                .map_err(|e| e.to_string())?
                .clone();
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
