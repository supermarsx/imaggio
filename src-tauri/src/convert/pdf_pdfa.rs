//! Native PDF/A-2b converter.
//!
//! Converts a PDF to PDF/A-2b conformance by:
//! 1. Adding XMP metadata with PDF/A identification
//! 2. Embedding sRGB ICC profile as OutputIntent
//! 3. Removing prohibited elements (JavaScript, actions, encryption)
//! 4. Verifying fonts are embedded (warning if not)

use lopdf::{Document, Object, Stream};
use std::path::Path;

use super::suffix_path;

static SRGB_ICC_PROFILE: &[u8] = include_bytes!("../../assets/sRGB.icc");

pub fn to_pdfa(input: &Path, compress_quality: Option<&str>) -> Result<Vec<String>, String> {
    let mut doc = Document::load(input).map_err(|e| format!("Failed to load PDF: {e}"))?;

    let suffix = match compress_quality {
        Some("screen") => "_pdfa_low",
        Some("ebook") => "_pdfa_med",
        Some("printer") => "_pdfa_high",
        _ => "_pdfa",
    };
    let output = suffix_path(input, suffix, None);

    // Step 1: Add XMP metadata
    add_xmp_metadata(&mut doc)?;

    // Step 2: Add sRGB ICC OutputIntent
    add_icc_output_intent(&mut doc)?;

    // Step 3: Remove prohibited elements
    remove_prohibited_elements(&mut doc);

    // Step 4: Verify fonts (log warnings but don't fail)
    let warnings = verify_fonts_embedded(&doc);
    for w in &warnings {
        log::warn!("PDF/A font warning: {w}");
    }

    // Optional compression
    if let Some(quality) = compress_quality {
        doc.prune_objects();
        doc.delete_zero_length_streams();
        doc.compress();

        if quality == "screen" || quality == "ebook" {
            let scale = if quality == "screen" { 0.25 } else { 0.5 };
            let _ = super::pdf_gs::compress_images_in_doc(&mut doc, scale);
        }
    }

    doc.save(&output).map_err(|e| format!("Failed to save PDF/A: {e}"))?;
    Ok(vec![output.to_string_lossy().to_string()])
}

fn add_xmp_metadata(doc: &mut Document) -> Result<(), String> {
    // Get existing metadata for dc:title, dc:creator if available
    let (title, creator) = get_info_fields(doc);

    let xmp = format!(
        r#"<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
  <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
    <rdf:Description rdf:about=""
      xmlns:dc="http://purl.org/dc/elements/1.1/"
      xmlns:pdfaid="http://www.aiim.org/pdfa/ns/id/"
      xmlns:xmp="http://ns.adobe.com/xap/1.0/">
      <pdfaid:part>2</pdfaid:part>
      <pdfaid:conformance>B</pdfaid:conformance>
      <dc:title>
        <rdf:Alt>
          <rdf:li xml:lang="x-default">{title}</rdf:li>
        </rdf:Alt>
      </dc:title>
      <dc:creator>
        <rdf:Seq>
          <rdf:li>{creator}</rdf:li>
        </rdf:Seq>
      </dc:creator>
    </rdf:Description>
  </rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>"#
    );

    let xmp_bytes = xmp.into_bytes();

    let mut dict = lopdf::Dictionary::new();
    dict.set(b"Type".to_vec(), Object::Name(b"Metadata".to_vec()));
    dict.set(b"Subtype".to_vec(), Object::Name(b"XML".to_vec()));
    dict.set(
        b"Length".to_vec(),
        Object::Integer(xmp_bytes.len() as i64),
    );

    let stream = Stream::new(dict, xmp_bytes);
    let metadata_id = doc.add_object(Object::Stream(stream));

    // Attach to catalog
    let catalog = doc
        .catalog_mut()
        .map_err(|e| format!("Failed to get catalog: {e}"))?;
    catalog.set(
        b"Metadata".to_vec(),
        Object::Reference(metadata_id),
    );

    Ok(())
}

fn get_info_fields(doc: &Document) -> (String, String) {
    let mut title = String::from("Untitled");
    let mut creator = String::from("Imaggio");

    if let Ok(info_ref) = doc.trailer.get(b"Info") {
        if let Ok(info_id) = info_ref.as_reference() {
            if let Ok(info_obj) = doc.get_object(info_id) {
                if let Ok(info_dict) = info_obj.as_dict() {
                    if let Ok(Object::String(bytes, _)) = info_dict.get(b"Title") {
                        let t = String::from_utf8_lossy(bytes).to_string();
                        if !t.is_empty() {
                            title = escape_xml(&t);
                        }
                    }
                    if let Ok(Object::String(bytes, _)) = info_dict.get(b"Creator") {
                        let c = String::from_utf8_lossy(bytes).to_string();
                        if !c.is_empty() {
                            creator = escape_xml(&c);
                        }
                    }
                }
            }
        }
    }

    (title, creator)
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn add_icc_output_intent(doc: &mut Document) -> Result<(), String> {
    // Create ICC profile stream
    let mut icc_dict = lopdf::Dictionary::new();
    icc_dict.set(
        b"Length".to_vec(),
        Object::Integer(SRGB_ICC_PROFILE.len() as i64),
    );
    icc_dict.set(b"N".to_vec(), Object::Integer(3)); // RGB = 3 components
    icc_dict.set(
        b"Filter".to_vec(),
        Object::Name(b"FlateDecode".to_vec()),
    );

    // Compress the ICC data
    let compressed = compress_data(SRGB_ICC_PROFILE);
    let icc_stream = Stream::new(icc_dict, compressed);
    let icc_id = doc.add_object(Object::Stream(icc_stream));

    // Create OutputIntent dictionary
    let mut output_intent = lopdf::Dictionary::new();
    output_intent.set(
        b"Type".to_vec(),
        Object::Name(b"OutputIntent".to_vec()),
    );
    output_intent.set(
        b"S".to_vec(),
        Object::Name(b"GTS_PDFA1".to_vec()),
    );
    output_intent.set(
        b"OutputConditionIdentifier".to_vec(),
        Object::String(
            b"sRGB IEC61966-2.1".to_vec(),
            lopdf::StringFormat::Literal,
        ),
    );
    output_intent.set(
        b"RegistryName".to_vec(),
        Object::String(
            b"http://www.color.org".to_vec(),
            lopdf::StringFormat::Literal,
        ),
    );
    output_intent.set(
        b"Info".to_vec(),
        Object::String(b"sRGB IEC61966-2.1".to_vec(), lopdf::StringFormat::Literal),
    );
    output_intent.set(
        b"DestOutputProfile".to_vec(),
        Object::Reference(icc_id),
    );

    let intent_id = doc.add_object(Object::Dictionary(output_intent));

    // Add to catalog's OutputIntents array
    let catalog = doc
        .catalog_mut()
        .map_err(|e| format!("Failed to get catalog: {e}"))?;

    let output_intents = if let Ok(existing) = catalog.get(b"OutputIntents") {
        if let Ok(arr) = existing.as_array() {
            let mut new_arr = arr.clone();
            new_arr.push(Object::Reference(intent_id));
            new_arr
        } else {
            vec![Object::Reference(intent_id)]
        }
    } else {
        vec![Object::Reference(intent_id)]
    };

    catalog.set(b"OutputIntents".to_vec(), Object::Array(output_intents));

    Ok(())
}

fn compress_data(data: &[u8]) -> Vec<u8> {
    use flate2::write::ZlibEncoder;
    use flate2::Compression;
    use std::io::Write;

    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
}

fn remove_prohibited_elements(doc: &mut Document) {
    // Remove JavaScript from Names dict
    if let Ok(catalog) = doc.catalog_mut() {
        // Remove /AA (additional actions) from catalog
        catalog.remove(b"AA");

        // Remove JavaScript from Names
        if let Ok(names_ref) = catalog.get(b"Names").cloned() {
            if let Ok(names_id) = names_ref.as_reference() {
                if let Ok(names_obj) = doc.get_object_mut(names_id) {
                    if let Object::Dictionary(ref mut dict) = names_obj {
                        dict.remove(b"JavaScript");
                    }
                }
            }
        }
    }

    // Remove /AA from all pages
    let page_ids: Vec<_> = doc.get_pages().values().copied().collect();
    for page_id in page_ids {
        if let Ok(page_obj) = doc.get_object_mut(page_id) {
            if let Object::Dictionary(ref mut dict) = page_obj {
                dict.remove(b"AA");
            }
        }
    }

    // Remove encryption
    doc.trailer.remove(b"Encrypt");
}

fn verify_fonts_embedded(doc: &Document) -> Vec<String> {
    let mut warnings = Vec::new();
    let pages = doc.get_pages();

    for (&page_num, &page_id) in &pages {
        let page_dict = match doc.get_object(page_id).and_then(|o| o.as_dict()) {
            Ok(d) => d,
            Err(_) => continue,
        };

        let resources = match page_dict.get(b"Resources").ok().and_then(|r| match r {
            Object::Reference(id) => doc.get_object(*id).ok(),
            other => Some(other),
        }) {
            Some(r) => match r.as_dict() {
                Ok(d) => d,
                Err(_) => continue,
            },
            None => continue,
        };

        let fonts = match resources.get(b"Font").ok().and_then(|f| match f {
            Object::Reference(id) => doc.get_object(*id).ok(),
            other => Some(other),
        }) {
            Some(f) => match f.as_dict() {
                Ok(d) => d,
                Err(_) => continue,
            },
            None => continue,
        };

        for (font_name, font_ref) in fonts.iter() {
            let font_obj = match font_ref {
                Object::Reference(id) => match doc.get_object(*id) {
                    Ok(o) => o,
                    Err(_) => continue,
                },
                other => other,
            };

            let font_dict = match font_obj.as_dict() {
                Ok(d) => d,
                Err(_) => continue,
            };

            // Check for FontDescriptor
            let descriptor = match font_dict.get(b"FontDescriptor").ok().and_then(|d| {
                match d {
                    Object::Reference(id) => doc.get_object(*id).ok(),
                    other => Some(other),
                }
                .and_then(|o| o.as_dict().ok())
            }) {
                Some(d) => d,
                None => {
                    // Type0, Type1, Type3 base fonts may not have descriptors
                    continue;
                }
            };

            let has_fontfile = descriptor.get(b"FontFile").is_ok()
                || descriptor.get(b"FontFile2").is_ok()
                || descriptor.get(b"FontFile3").is_ok();

            if !has_fontfile {
                let name_str = String::from_utf8_lossy(font_name);
                warnings.push(format!(
                    "Page {page_num}: Font '{name_str}' is not embedded"
                ));
            }
        }
    }

    warnings
}
