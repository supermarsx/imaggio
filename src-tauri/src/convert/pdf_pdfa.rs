//! PDF/A and PDF/X converter.
//!
//! Supports all major PDF/A conformance levels and PDF/X print profiles:
//!
//! PDF/A (archival):
//!   - PDF/A-1b, PDF/A-1a (ISO 19005-1, based on PDF 1.4)
//!   - PDF/A-2a, PDF/A-2b, PDF/A-2u (ISO 19005-2, based on PDF 1.7)
//!   - PDF/A-3 (ISO 19005-3, based on PDF 1.7, allows arbitrary attachments)
//!   - PDF/A-4, PDF/A-4f, PDF/A-4e (ISO 19005-4, based on PDF 2.0)
//!
//! PDF/X (print production):
//!   - PDF/X-1a:2003 (CMYK/Gray only)
//!   - PDF/X-3:2003 (color-managed, RGB/CMYK/Lab)
//!   - PDF/X-4 (transparency preserved, embedded ICC required)
//!
//! Pipeline for each level:
//! 1. Add XMP metadata with standard identification
//! 2. Embed sRGB ICC profile as OutputIntent
//! 3. Remove prohibited elements (JavaScript, actions, encryption)
//! 4. Level-specific processing (transparency removal, structure verification)
//! 5. Optional compression

use lopdf::{Document, Object, Stream};
use std::path::Path;

use super::suffix_path;

static SRGB_ICC_PROFILE: &[u8] = include_bytes!("../../assets/sRGB.icc");

// ── Public enums ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub enum PdfaLevel {
    A1b,
    A1a,
    A2a,
    A2b,
    A2u,
    A3,
    A4,
    A4f,
    A4e,
}

#[derive(Debug, Clone, Copy)]
pub enum PdfxLevel {
    X1a,
    X3,
    X4,
}

impl PdfaLevel {
    fn part(&self) -> u8 {
        match self {
            Self::A1b | Self::A1a => 1,
            Self::A2a | Self::A2b | Self::A2u => 2,
            Self::A3 => 3,
            Self::A4 | Self::A4f | Self::A4e => 4,
        }
    }

    /// Returns the conformance letter, or None for PDF/A-4 base level.
    fn conformance(&self) -> Option<&'static str> {
        match self {
            Self::A1b | Self::A2b => Some("B"),
            Self::A1a | Self::A2a => Some("A"),
            Self::A2u => Some("U"),
            Self::A3 => Some("B"),
            Self::A4 => None,
            Self::A4f => Some("F"),
            Self::A4e => Some("E"),
        }
    }

    fn suffix(&self) -> &'static str {
        match self {
            Self::A1b => "_pdfa1b",
            Self::A1a => "_pdfa1a",
            Self::A2a => "_pdfa2a",
            Self::A2b => "_pdfa2b",
            Self::A2u => "_pdfa2u",
            Self::A3 => "_pdfa3",
            Self::A4 => "_pdfa4",
            Self::A4f => "_pdfa4f",
            Self::A4e => "_pdfa4e",
        }
    }

    fn is_v1(&self) -> bool {
        matches!(self, Self::A1b | Self::A1a)
    }

    fn requires_tagged(&self) -> bool {
        matches!(self, Self::A1a | Self::A2a)
    }

    fn requires_unicode(&self) -> bool {
        matches!(self, Self::A1a | Self::A2a | Self::A2u)
    }
}

impl PdfxLevel {
    fn version_string(&self) -> &'static str {
        match self {
            Self::X1a => "PDF/X-1a:2003",
            Self::X3 => "PDF/X-3:2003",
            Self::X4 => "PDF/X-4",
        }
    }

    fn suffix(&self) -> &'static str {
        match self {
            Self::X1a => "_pdfx1a",
            Self::X3 => "_pdfx3",
            Self::X4 => "_pdfx4",
        }
    }
}

// ── Internal XMP profile type ─────────────────────────────────────────────

enum XmpProfile<'a> {
    PdfA { part: u8, conformance: Option<&'a str> },
    PdfX { version: &'a str },
}

// ── Public API ────────────────────────────────────────────────────────────

pub fn to_pdfa(
    input: &Path,
    level: PdfaLevel,
    compress_quality: Option<&str>,
) -> Result<Vec<String>, String> {
    let mut doc = Document::load(input).map_err(|e| format!("Failed to load PDF: {e}"))?;

    let compress_suffix = match compress_quality {
        Some("screen") => "_low",
        Some("ebook") => "_med",
        Some("printer") => "_high",
        _ => "",
    };
    let output = suffix_path(input, &format!("{}{compress_suffix}", level.suffix()), None);

    // Step 1: XMP metadata
    let profile = XmpProfile::PdfA {
        part: level.part(),
        conformance: level.conformance(),
    };
    add_xmp_metadata(&mut doc, &profile)?;

    // Step 2: ICC OutputIntent
    add_icc_output_intent(&mut doc, b"GTS_PDFA1")?;

    // Step 3: Remove prohibited elements
    remove_prohibited_elements(&mut doc, level.is_v1());

    // Step 4: Level-specific processing
    if level.is_v1() {
        remove_transparency(&mut doc);
    }

    // Step 5: Verification (warnings only, don't fail)
    let font_warnings = verify_fonts_embedded(&doc);
    for w in &font_warnings {
        log::warn!("PDF/A font warning: {w}");
    }

    if level.requires_tagged() {
        let tag_warnings = verify_tagged_structure(&doc);
        for w in &tag_warnings {
            log::warn!("PDF/A structure warning: {w}");
        }
    }

    if level.requires_unicode() {
        let unicode_warnings = verify_unicode_mapping(&doc);
        for w in &unicode_warnings {
            log::warn!("PDF/A Unicode warning: {w}");
        }
    }

    // Step 6: Optional compression
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

pub fn to_pdfx(input: &Path, level: PdfxLevel) -> Result<Vec<String>, String> {
    let mut doc = Document::load(input).map_err(|e| format!("Failed to load PDF: {e}"))?;

    let output = suffix_path(input, level.suffix(), None);

    // Step 1: XMP metadata
    let profile = XmpProfile::PdfX {
        version: level.version_string(),
    };
    add_xmp_metadata(&mut doc, &profile)?;

    // Step 2: ICC OutputIntent (PDF/X uses GTS_PDFX)
    add_icc_output_intent(&mut doc, b"GTS_PDFX")?;

    // Step 3: Remove prohibited elements (same as PDF/A-2+)
    remove_prohibited_elements(&mut doc, false);

    // Step 4: Level-specific checks
    if matches!(level, PdfxLevel::X1a) {
        let cmyk_warnings = check_cmyk_only(&doc);
        for w in &cmyk_warnings {
            log::warn!("PDF/X-1a color warning: {w}");
        }
    }

    let font_warnings = verify_fonts_embedded(&doc);
    for w in &font_warnings {
        log::warn!("PDF/X font warning: {w}");
    }

    // Step 5: Structural optimization
    doc.prune_objects();
    doc.delete_zero_length_streams();
    doc.compress();

    doc.save(&output).map_err(|e| format!("Failed to save PDF/X: {e}"))?;
    Ok(vec![output.to_string_lossy().to_string()])
}

// ── XMP metadata ──────────────────────────────────────────────────────────

fn add_xmp_metadata(doc: &mut Document, profile: &XmpProfile) -> Result<(), String> {
    let (title, creator) = get_info_fields(doc);

    let (ns_decl, standard_props) = match profile {
        XmpProfile::PdfA { part, conformance } => {
            let conf_line = match conformance {
                Some(c) => format!("      <pdfaid:conformance>{c}</pdfaid:conformance>"),
                None => String::new(),
            };
            (
                "xmlns:pdfaid=\"http://www.aiim.org/pdfa/ns/id/\"".to_string(),
                format!(
                    "      <pdfaid:part>{part}</pdfaid:part>\n{conf_line}"
                ),
            )
        }
        XmpProfile::PdfX { version } => (
            "xmlns:pdfxid=\"http://www.npes.org/pdfx/ns/id/\"".to_string(),
            format!("      <pdfxid:GTS_PDFXVersion>{version}</pdfxid:GTS_PDFXVersion>"),
        ),
    };

    let xmp = format!(
        r#"<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
  <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
    <rdf:Description rdf:about=""
      xmlns:dc="http://purl.org/dc/elements/1.1/"
      xmlns:xmp="http://ns.adobe.com/xap/1.0/"
      {ns_decl}>
{standard_props}
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

// ── ICC OutputIntent ──────────────────────────────────────────────────────

fn add_icc_output_intent(doc: &mut Document, intent_subtype: &[u8]) -> Result<(), String> {
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
        Object::Name(intent_subtype.to_vec()),
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

// ── Prohibited element removal ────────────────────────────────────────────

fn remove_prohibited_elements(doc: &mut Document, strict_v1: bool) {
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
    for page_id in &page_ids {
        if let Ok(page_obj) = doc.get_object_mut(*page_id) {
            if let Object::Dictionary(ref mut dict) = page_obj {
                dict.remove(b"AA");
            }
        }
    }

    // Remove encryption
    doc.trailer.remove(b"Encrypt");

    // PDF/A-1 is stricter: also remove /OCProperties (optional content / layers)
    if strict_v1 {
        if let Ok(catalog) = doc.catalog_mut() {
            catalog.remove(b"OCProperties");
        }
    }
}

// ── PDF/A-1 specific: transparency removal ────────────────────────────────

fn remove_transparency(doc: &mut Document) {
    let page_ids: Vec<_> = doc.get_pages().values().copied().collect();
    for page_id in page_ids {
        if let Ok(page_obj) = doc.get_object_mut(page_id) {
            if let Object::Dictionary(ref mut dict) = page_obj {
                // Remove /Group dict (transparency group) from pages
                dict.remove(b"Group");
            }
        }
    }
}

// ── Font verification ─────────────────────────────────────────────────────

fn verify_fonts_embedded(doc: &Document) -> Vec<String> {
    let mut warnings = Vec::new();
    let pages = doc.get_pages();

    for (&page_num, &page_id) in &pages {
        let fonts = match get_page_fonts(doc, page_id) {
            Some(f) => f,
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

            let descriptor = match font_dict.get(b"FontDescriptor").ok().and_then(|d| {
                match d {
                    Object::Reference(id) => doc.get_object(*id).ok(),
                    other => Some(other),
                }
                .and_then(|o| o.as_dict().ok())
            }) {
                Some(d) => d,
                None => continue,
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

// ── Tagged structure verification ─────────────────────────────────────────

fn verify_tagged_structure(doc: &Document) -> Vec<String> {
    let mut warnings = Vec::new();

    let catalog = match doc.catalog() {
        Ok(c) => c,
        Err(_) => {
            warnings.push("Cannot read catalog to verify tagged structure".to_string());
            return warnings;
        }
    };

    // Check for /MarkInfo -> /Marked true
    let has_mark_info = catalog
        .get(b"MarkInfo")
        .ok()
        .and_then(|obj| {
            let dict = match obj {
                Object::Reference(id) => doc.get_object(*id).ok()?.as_dict().ok(),
                Object::Dictionary(d) => Some(d),
                _ => None,
            };
            dict.and_then(|d| {
                d.get(b"Marked")
                    .ok()
                    .and_then(|v| match v {
                        Object::Boolean(b) => Some(*b),
                        _ => None,
                    })
            })
        })
        .unwrap_or(false);

    if !has_mark_info {
        warnings.push("Document is not tagged (missing /MarkInfo with /Marked true). PDF/A-1a and PDF/A-2a require tagged PDF structure for accessibility.".to_string());
    }

    // Check for /StructTreeRoot
    if catalog.get(b"StructTreeRoot").is_err() {
        warnings.push(
            "Document has no structure tree (/StructTreeRoot). Tagged PDF requires a logical structure tree.".to_string(),
        );
    }

    warnings
}

// ── Unicode mapping verification ──────────────────────────────────────────

fn verify_unicode_mapping(doc: &Document) -> Vec<String> {
    let mut warnings = Vec::new();
    let pages = doc.get_pages();

    for (&page_num, &page_id) in &pages {
        let fonts = match get_page_fonts(doc, page_id) {
            Some(f) => f,
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

            // Check for /ToUnicode CMap
            if font_dict.get(b"ToUnicode").is_err() {
                // Type1 standard 14 fonts and simple encodings may not need ToUnicode,
                // but for strict compliance we warn
                let font_type = font_dict
                    .get(b"Subtype")
                    .ok()
                    .and_then(|o| o.as_name().ok())
                    .map(|n| n.to_vec());

                // Skip Type0 composite fonts that use CIDFont with Identity-H encoding
                // (they should have ToUnicode but many don't in practice)
                let name_str = String::from_utf8_lossy(font_name);
                let type_str = font_type
                    .as_deref()
                    .map(|t| String::from_utf8_lossy(t).to_string())
                    .unwrap_or_default();

                warnings.push(format!(
                    "Page {page_num}: Font '{name_str}' ({type_str}) has no /ToUnicode mapping"
                ));
            }
        }
    }

    warnings
}

// ── PDF/X-1a color space check ────────────────────────────────────────────

fn check_cmyk_only(doc: &Document) -> Vec<String> {
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

        // Check /ColorSpace dictionary for RGB entries
        if let Ok(cs_obj) = resources.get(b"ColorSpace") {
            let cs_dict = match cs_obj {
                Object::Reference(id) => doc.get_object(*id).ok().and_then(|o| o.as_dict().ok()),
                Object::Dictionary(d) => Some(d),
                _ => None,
            };

            if let Some(cs_dict) = cs_dict {
                for (name, value) in cs_dict.iter() {
                    let cs_name = resolve_colorspace_name(value, doc);
                    if cs_name.as_deref() == Some("DeviceRGB")
                        || cs_name.as_deref() == Some("CalRGB")
                    {
                        let name_str = String::from_utf8_lossy(name);
                        warnings.push(format!(
                            "Page {page_num}: Color space '{name_str}' uses RGB (PDF/X-1a requires CMYK/Gray only)"
                        ));
                    }
                }
            }
        }

        // Check XObject images for RGB color spaces
        if let Ok(xobj_ref) = resources.get(b"XObject") {
            let xobjects = match xobj_ref {
                Object::Reference(id) => doc.get_object(*id).ok().and_then(|o| o.as_dict().ok()),
                Object::Dictionary(d) => Some(d),
                _ => None,
            };

            if let Some(xobjects) = xobjects {
                for (name, obj_ref) in xobjects.iter() {
                    let obj = match obj_ref {
                        Object::Reference(id) => match doc.get_object(*id) {
                            Ok(o) => o,
                            Err(_) => continue,
                        },
                        other => other,
                    };

                    if let Object::Stream(stream) = obj {
                        let is_image = stream
                            .dict
                            .get(b"Subtype")
                            .ok()
                            .and_then(|o| o.as_name().ok())
                            .map(|n| n == b"Image")
                            .unwrap_or(false);

                        if is_image {
                            let cs = stream
                                .dict
                                .get(b"ColorSpace")
                                .ok()
                                .and_then(|o| o.as_name().ok())
                                .map(|n| String::from_utf8_lossy(n).to_string());

                            if cs.as_deref() == Some("DeviceRGB") {
                                let name_str = String::from_utf8_lossy(name);
                                warnings.push(format!(
                                    "Page {page_num}: Image XObject '{name_str}' uses DeviceRGB (PDF/X-1a requires CMYK/Gray only)"
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    warnings
}

fn resolve_colorspace_name(obj: &Object, doc: &Document) -> Option<String> {
    match obj {
        Object::Name(n) => Some(String::from_utf8_lossy(n).to_string()),
        Object::Array(arr) => {
            // [/ICCBased ref] or [/CalRGB dict] etc — first element is the CS name
            arr.first()
                .and_then(|o| o.as_name().ok())
                .map(|n| String::from_utf8_lossy(n).to_string())
        }
        Object::Reference(id) => doc
            .get_object(*id)
            .ok()
            .and_then(|o| resolve_colorspace_name(o, doc)),
        _ => None,
    }
}

// ── Shared helpers ────────────────────────────────────────────────────────

fn get_page_fonts(doc: &Document, page_id: lopdf::ObjectId) -> Option<lopdf::Dictionary> {
    let page_dict = doc.get_object(page_id).ok()?.as_dict().ok()?;

    let resources = match page_dict.get(b"Resources").ok()? {
        Object::Reference(id) => doc.get_object(*id).ok()?,
        other => other,
    };
    let resources = resources.as_dict().ok()?;

    let fonts = match resources.get(b"Font").ok()? {
        Object::Reference(id) => doc.get_object(*id).ok()?,
        other => other,
    };
    fonts.as_dict().ok().cloned()
}
