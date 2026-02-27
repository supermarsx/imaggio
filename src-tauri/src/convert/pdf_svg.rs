//! PDF → SVG converter
//!
//! Parses PDF content streams and translates drawing operators to SVG elements.
//! Handles paths, colors, text, graphics state, and image XObjects.

use lopdf::{Document, Object};
use std::fmt::Write as FmtWrite;
use std::path::Path;

use super::suffix_path;

#[derive(Clone, Debug)]
struct Color {
    r: f64,
    g: f64,
    b: f64,
}

impl Color {
    fn black() -> Self {
        Self {
            r: 0.0,
            g: 0.0,
            b: 0.0,
        }
    }

    fn to_css(&self) -> String {
        format!(
            "rgb({},{},{})",
            (self.r * 255.0).round() as u8,
            (self.g * 255.0).round() as u8,
            (self.b * 255.0).round() as u8,
        )
    }

    fn from_gray(g: f64) -> Self {
        Self { r: g, g, b: g }
    }

    fn from_rgb(r: f64, g: f64, b: f64) -> Self {
        Self { r, g, b }
    }

    fn from_cmyk(c: f64, m: f64, y: f64, k: f64) -> Self {
        Self {
            r: (1.0 - c) * (1.0 - k),
            g: (1.0 - m) * (1.0 - k),
            b: (1.0 - y) * (1.0 - k),
        }
    }
}

#[derive(Clone, Debug)]
struct GraphicsState {
    fill_color: Color,
    stroke_color: Color,
    line_width: f64,
    line_cap: u32,
    line_join: u32,
    dash_array: Vec<f64>,
    dash_phase: f64,
    font_name: String,
    font_size: f64,
    text_matrix: [f64; 6],
    text_line_matrix: [f64; 6],
    ctm: [f64; 6],
}

impl Default for GraphicsState {
    fn default() -> Self {
        Self {
            fill_color: Color::black(),
            stroke_color: Color::black(),
            line_width: 1.0,
            line_cap: 0,
            line_join: 0,
            dash_array: vec![],
            dash_phase: 0.0,
            font_name: String::new(),
            font_size: 12.0,
            text_matrix: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            text_line_matrix: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            ctm: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        }
    }
}

fn multiply_matrices(a: &[f64; 6], b: &[f64; 6]) -> [f64; 6] {
    [
        a[0] * b[0] + a[1] * b[2],
        a[0] * b[1] + a[1] * b[3],
        a[2] * b[0] + a[3] * b[2],
        a[2] * b[1] + a[3] * b[3],
        a[4] * b[0] + a[5] * b[2] + b[4],
        a[4] * b[1] + a[5] * b[3] + b[5],
    ]
}

fn get_number(obj: &Object) -> Option<f64> {
    match obj {
        Object::Integer(i) => Some(*i as f64),
        Object::Real(f) => Some(*f as f64),
        _ => None,
    }
}

fn decode_pdf_string(bytes: &[u8]) -> String {
    // Simple decoding: try UTF-16BE (if BOM present) then Latin-1
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        let chars: Vec<u16> = bytes[2..]
            .chunks(2)
            .filter_map(|c| {
                if c.len() == 2 {
                    Some(u16::from_be_bytes([c[0], c[1]]))
                } else {
                    None
                }
            })
            .collect();
        String::from_utf16_lossy(&chars)
    } else {
        // PDFDocEncoding (close enough to Latin-1 for most text)
        bytes.iter().map(|&b| b as char).collect()
    }
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub fn to_svg(input: &Path) -> Result<Vec<String>, String> {
    let doc = Document::load(input).map_err(|e| format!("Failed to load PDF: {e}"))?;
    let pages = doc.get_pages();

    if pages.is_empty() {
        return Err("PDF has no pages".to_string());
    }

    let mut outputs = Vec::new();
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");
    let output_dir = input.parent().unwrap_or_else(|| Path::new("."));

    let mut sorted_pages: Vec<_> = pages.iter().collect();
    sorted_pages.sort_by_key(|(&num, _)| num);

    for (&page_num, &page_id) in &sorted_pages {
        let page_dict = doc
            .get_object(page_id)
            .and_then(|o| o.as_dict())
            .map_err(|e| format!("Failed to get page dict: {e}"))?;

        // Get MediaBox for page dimensions
        let (width, height) = get_media_box(page_dict, &doc);

        // Get page resources
        let resources = get_page_resources(page_dict, &doc);

        // Get content stream
        let content_data = get_content_stream(page_dict, &doc)?;

        // Parse and render to SVG
        let svg_body = render_content_to_svg(&content_data, &resources, &doc, width, height);

        let svg = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink"
     width="{width}" height="{height}" viewBox="0 0 {width} {height}">
<g transform="translate(0,{height}) scale(1,-1)">
{svg_body}
</g>
</svg>"#
        );

        let output_path = if sorted_pages.len() == 1 {
            suffix_path(input, "", Some("svg"))
        } else {
            output_dir.join(format!("{stem}-{page_num:03}.svg"))
        };

        std::fs::write(&output_path, &svg).map_err(|e| format!("Failed to write SVG: {e}"))?;
        outputs.push(output_path.to_string_lossy().to_string());
    }

    Ok(outputs)
}

fn get_media_box(page_dict: &lopdf::Dictionary, doc: &Document) -> (f64, f64) {
    if let Ok(mb) = page_dict.get(b"MediaBox") {
        if let Ok(arr) = resolve_object(mb, doc).and_then(|o| o.as_array().map(|a| a.clone())) {
            if arr.len() >= 4 {
                let w = get_number(&arr[2]).unwrap_or(612.0) - get_number(&arr[0]).unwrap_or(0.0);
                let h = get_number(&arr[3]).unwrap_or(792.0) - get_number(&arr[1]).unwrap_or(0.0);
                return (w, h);
            }
        }
    }
    (612.0, 792.0) // default letter size
}

fn resolve_object<'a>(obj: &'a Object, doc: &'a Document) -> Result<&'a Object, lopdf::Error> {
    match obj {
        Object::Reference(id) => doc.get_object(*id),
        _ => Ok(obj),
    }
}

fn get_page_resources<'a>(
    page_dict: &'a lopdf::Dictionary,
    doc: &'a Document,
) -> Option<&'a lopdf::Dictionary> {
    page_dict
        .get(b"Resources")
        .ok()
        .and_then(|r| match r {
            Object::Reference(id) => doc.get_object(*id).ok(),
            Object::Dictionary(_) => Some(r),
            _ => None,
        })
        .and_then(|o| o.as_dict().ok())
}

fn get_content_stream(page_dict: &lopdf::Dictionary, doc: &Document) -> Result<Vec<u8>, String> {
    let contents = page_dict
        .get(b"Contents")
        .map_err(|_| "Page has no content stream".to_string())?;

    match contents {
        Object::Reference(id) => {
            let obj = doc
                .get_object(*id)
                .map_err(|e| format!("Failed to get content stream: {e}"))?;
            match obj {
                Object::Stream(stream) => {
                    stream.decompressed_content().map_err(|e| format!("Failed to decompress: {e}"))
                }
                _ => Err("Content is not a stream".to_string()),
            }
        }
        Object::Array(arr) => {
            let mut combined = Vec::new();
            for item in arr {
                let obj = match item {
                    Object::Reference(id) => doc
                        .get_object(*id)
                        .map_err(|e| format!("Failed to get stream: {e}"))?,
                    other => other,
                };
                if let Object::Stream(stream) = obj {
                    let data = stream
                        .decompressed_content()
                        .map_err(|e| format!("Failed to decompress: {e}"))?;
                    combined.extend_from_slice(&data);
                    combined.push(b' ');
                }
            }
            Ok(combined)
        }
        Object::Stream(stream) => {
            stream.decompressed_content().map_err(|e| format!("Failed to decompress: {e}"))
        }
        _ => Err("Unexpected content type".to_string()),
    }
}

fn render_content_to_svg(
    content: &[u8],
    resources: &Option<&lopdf::Dictionary>,
    doc: &Document,
    _page_width: f64,
    _page_height: f64,
) -> String {
    let mut svg = String::new();
    let mut state = GraphicsState::default();
    let mut state_stack: Vec<GraphicsState> = Vec::new();
    let mut path_data = String::new();
    let mut in_text = false;
    let mut operands: Vec<Object> = Vec::new();

    let tokens = tokenize_content_stream(content);

    for token in &tokens {
        match token {
            Token::Operand(obj) => {
                operands.push(obj.clone());
            }
            Token::Operator(op) => {
                match op.as_str() {
                    // Graphics state
                    "q" => state_stack.push(state.clone()),
                    "Q" => {
                        if let Some(s) = state_stack.pop() {
                            state = s;
                        }
                    }
                    "cm" => {
                        if operands.len() >= 6 {
                            let m = [
                                get_number(&operands[0]).unwrap_or(1.0),
                                get_number(&operands[1]).unwrap_or(0.0),
                                get_number(&operands[2]).unwrap_or(0.0),
                                get_number(&operands[3]).unwrap_or(1.0),
                                get_number(&operands[4]).unwrap_or(0.0),
                                get_number(&operands[5]).unwrap_or(0.0),
                            ];
                            state.ctm = multiply_matrices(&m, &state.ctm);
                        }
                    }
                    "w" => {
                        if let Some(w) = operands.first().and_then(get_number) {
                            state.line_width = w;
                        }
                    }
                    "J" => {
                        if let Some(Object::Integer(j)) = operands.first() {
                            state.line_cap = *j as u32;
                        }
                    }
                    "j" => {
                        if let Some(Object::Integer(j)) = operands.first() {
                            state.line_join = *j as u32;
                        }
                    }
                    "d" => {
                        if operands.len() >= 2 {
                            if let Object::Array(arr) = &operands[0] {
                                state.dash_array =
                                    arr.iter().filter_map(get_number).collect();
                            }
                            state.dash_phase = get_number(&operands[1]).unwrap_or(0.0);
                        }
                    }

                    // Path construction
                    "m" => {
                        if operands.len() >= 2 {
                            let x = get_number(&operands[0]).unwrap_or(0.0);
                            let y = get_number(&operands[1]).unwrap_or(0.0);
                            let _ = write!(path_data, "M{x} {y} ");
                        }
                    }
                    "l" => {
                        if operands.len() >= 2 {
                            let x = get_number(&operands[0]).unwrap_or(0.0);
                            let y = get_number(&operands[1]).unwrap_or(0.0);
                            let _ = write!(path_data, "L{x} {y} ");
                        }
                    }
                    "c" => {
                        if operands.len() >= 6 {
                            let vals: Vec<f64> =
                                operands.iter().take(6).filter_map(get_number).collect();
                            if vals.len() == 6 {
                                let _ = write!(
                                    path_data,
                                    "C{} {},{} {},{} {} ",
                                    vals[0], vals[1], vals[2], vals[3], vals[4], vals[5]
                                );
                            }
                        }
                    }
                    "v" => {
                        if operands.len() >= 4 {
                            let vals: Vec<f64> =
                                operands.iter().take(4).filter_map(get_number).collect();
                            if vals.len() == 4 {
                                // v uses current point as first control point
                                let _ = write!(
                                    path_data,
                                    "S{} {},{} {} ",
                                    vals[0], vals[1], vals[2], vals[3]
                                );
                            }
                        }
                    }
                    "y" => {
                        if operands.len() >= 4 {
                            let vals: Vec<f64> =
                                operands.iter().take(4).filter_map(get_number).collect();
                            if vals.len() == 4 {
                                let _ = write!(
                                    path_data,
                                    "C{} {},{} {},{} {} ",
                                    vals[0], vals[1], vals[2], vals[3], vals[2], vals[3]
                                );
                            }
                        }
                    }
                    "h" => {
                        path_data.push_str("Z ");
                    }
                    "re" => {
                        if operands.len() >= 4 {
                            let x = get_number(&operands[0]).unwrap_or(0.0);
                            let y = get_number(&operands[1]).unwrap_or(0.0);
                            let w = get_number(&operands[2]).unwrap_or(0.0);
                            let h = get_number(&operands[3]).unwrap_or(0.0);
                            let _ = write!(
                                path_data,
                                "M{x} {y} L{} {y} L{} {} L{x} {} Z ",
                                x + w,
                                x + w,
                                y + h,
                                y + h
                            );
                        }
                    }

                    // Path painting
                    "S" => {
                        if !path_data.is_empty() {
                            emit_path(&mut svg, &path_data, &state, false, true);
                            path_data.clear();
                        }
                    }
                    "s" => {
                        path_data.push_str("Z ");
                        if !path_data.is_empty() {
                            emit_path(&mut svg, &path_data, &state, false, true);
                            path_data.clear();
                        }
                    }
                    "f" | "F" => {
                        if !path_data.is_empty() {
                            emit_path(&mut svg, &path_data, &state, true, false);
                            path_data.clear();
                        }
                    }
                    "f*" => {
                        if !path_data.is_empty() {
                            let _ = write!(
                                svg,
                                r#"<path d="{}" fill="{}" fill-rule="evenodd" stroke="none" transform="matrix({},{},{},{},{},{})"/>"#,
                                path_data.trim(),
                                state.fill_color.to_css(),
                                state.ctm[0], state.ctm[1], state.ctm[2], state.ctm[3],
                                state.ctm[4], state.ctm[5],
                            );
                            svg.push('\n');
                            path_data.clear();
                        }
                    }
                    "B" => {
                        if !path_data.is_empty() {
                            emit_path(&mut svg, &path_data, &state, true, true);
                            path_data.clear();
                        }
                    }
                    "B*" => {
                        if !path_data.is_empty() {
                            let _ = write!(
                                svg,
                                r#"<path d="{}" fill="{}" fill-rule="evenodd" stroke="{}" stroke-width="{}" transform="matrix({},{},{},{},{},{})"/>"#,
                                path_data.trim(),
                                state.fill_color.to_css(),
                                state.stroke_color.to_css(),
                                state.line_width,
                                state.ctm[0], state.ctm[1], state.ctm[2], state.ctm[3],
                                state.ctm[4], state.ctm[5],
                            );
                            svg.push('\n');
                            path_data.clear();
                        }
                    }
                    "b" => {
                        path_data.push_str("Z ");
                        if !path_data.is_empty() {
                            emit_path(&mut svg, &path_data, &state, true, true);
                            path_data.clear();
                        }
                    }
                    "n" => {
                        path_data.clear();
                    }

                    // Color operators
                    "g" => {
                        if let Some(g) = operands.first().and_then(get_number) {
                            state.fill_color = Color::from_gray(g);
                        }
                    }
                    "G" => {
                        if let Some(g) = operands.first().and_then(get_number) {
                            state.stroke_color = Color::from_gray(g);
                        }
                    }
                    "rg" => {
                        if operands.len() >= 3 {
                            let r = get_number(&operands[0]).unwrap_or(0.0);
                            let g = get_number(&operands[1]).unwrap_or(0.0);
                            let b = get_number(&operands[2]).unwrap_or(0.0);
                            state.fill_color = Color::from_rgb(r, g, b);
                        }
                    }
                    "RG" => {
                        if operands.len() >= 3 {
                            let r = get_number(&operands[0]).unwrap_or(0.0);
                            let g = get_number(&operands[1]).unwrap_or(0.0);
                            let b = get_number(&operands[2]).unwrap_or(0.0);
                            state.stroke_color = Color::from_rgb(r, g, b);
                        }
                    }
                    "k" => {
                        if operands.len() >= 4 {
                            let c = get_number(&operands[0]).unwrap_or(0.0);
                            let m = get_number(&operands[1]).unwrap_or(0.0);
                            let y = get_number(&operands[2]).unwrap_or(0.0);
                            let k = get_number(&operands[3]).unwrap_or(0.0);
                            state.fill_color = Color::from_cmyk(c, m, y, k);
                        }
                    }
                    "K" => {
                        if operands.len() >= 4 {
                            let c = get_number(&operands[0]).unwrap_or(0.0);
                            let m = get_number(&operands[1]).unwrap_or(0.0);
                            let y = get_number(&operands[2]).unwrap_or(0.0);
                            let k = get_number(&operands[3]).unwrap_or(0.0);
                            state.stroke_color = Color::from_cmyk(c, m, y, k);
                        }
                    }
                    "cs" | "CS" | "sc" | "SC" | "scn" | "SCN" => {
                        // Named color space operators — handle common cases
                        // For sc/SC/scn/SCN with numeric args, treat like rg/RG/g/G
                        let nums: Vec<f64> = operands.iter().filter_map(get_number).collect();
                        let color = match nums.len() {
                            1 => Some(Color::from_gray(nums[0])),
                            3 => Some(Color::from_rgb(nums[0], nums[1], nums[2])),
                            4 => Some(Color::from_cmyk(nums[0], nums[1], nums[2], nums[3])),
                            _ => None,
                        };
                        if let Some(c) = color {
                            if op == "sc" || op == "scn" || op == "cs" {
                                state.fill_color = c;
                            } else {
                                state.stroke_color = c;
                            }
                        }
                    }

                    // Text operators
                    "BT" => {
                        in_text = true;
                        state.text_matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
                        state.text_line_matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
                    }
                    "ET" => {
                        in_text = false;
                    }
                    "Tf" => {
                        if operands.len() >= 2 {
                            if let Object::Name(ref name) = operands[0] {
                                state.font_name =
                                    String::from_utf8_lossy(name).to_string();
                            }
                            state.font_size = get_number(&operands[1]).unwrap_or(12.0);
                        }
                    }
                    "Tm" => {
                        if operands.len() >= 6 {
                            state.text_matrix = [
                                get_number(&operands[0]).unwrap_or(1.0),
                                get_number(&operands[1]).unwrap_or(0.0),
                                get_number(&operands[2]).unwrap_or(0.0),
                                get_number(&operands[3]).unwrap_or(1.0),
                                get_number(&operands[4]).unwrap_or(0.0),
                                get_number(&operands[5]).unwrap_or(0.0),
                            ];
                            state.text_line_matrix = state.text_matrix;
                        }
                    }
                    "Td" => {
                        if operands.len() >= 2 {
                            let tx = get_number(&operands[0]).unwrap_or(0.0);
                            let ty = get_number(&operands[1]).unwrap_or(0.0);
                            let translate = [1.0, 0.0, 0.0, 1.0, tx, ty];
                            state.text_line_matrix =
                                multiply_matrices(&translate, &state.text_line_matrix);
                            state.text_matrix = state.text_line_matrix;
                        }
                    }
                    "TD" => {
                        if operands.len() >= 2 {
                            let tx = get_number(&operands[0]).unwrap_or(0.0);
                            let ty = get_number(&operands[1]).unwrap_or(0.0);
                            let translate = [1.0, 0.0, 0.0, 1.0, tx, ty];
                            state.text_line_matrix =
                                multiply_matrices(&translate, &state.text_line_matrix);
                            state.text_matrix = state.text_line_matrix;
                        }
                    }
                    "T*" => {
                        // Move to start of next text line (same as 0 -TL Td)
                        let translate = [1.0, 0.0, 0.0, 1.0, 0.0, -state.font_size];
                        state.text_line_matrix =
                            multiply_matrices(&translate, &state.text_line_matrix);
                        state.text_matrix = state.text_line_matrix;
                    }
                    "Tj" => {
                        if in_text {
                            if let Some(Object::String(ref bytes, _)) = operands.first() {
                                let text = decode_pdf_string(bytes);
                                emit_text(&mut svg, &text, &state);
                            }
                        }
                    }
                    "TJ" => {
                        if in_text {
                            if let Some(Object::Array(ref arr)) = operands.first() {
                                let mut combined = String::new();
                                for item in arr {
                                    if let Object::String(ref bytes, _) = item {
                                        combined.push_str(&decode_pdf_string(bytes));
                                    }
                                    // Numeric kerning adjustments are ignored for SVG
                                }
                                if !combined.is_empty() {
                                    emit_text(&mut svg, &combined, &state);
                                }
                            }
                        }
                    }
                    "'" => {
                        // Move to next line and show text
                        let translate = [1.0, 0.0, 0.0, 1.0, 0.0, -state.font_size];
                        state.text_line_matrix =
                            multiply_matrices(&translate, &state.text_line_matrix);
                        state.text_matrix = state.text_line_matrix;
                        if in_text {
                            if let Some(Object::String(ref bytes, _)) = operands.first() {
                                let text = decode_pdf_string(bytes);
                                emit_text(&mut svg, &text, &state);
                            }
                        }
                    }

                    // XObject (images, forms)
                    "Do" => {
                        if let Some(Object::Name(ref name)) = operands.first() {
                            emit_xobject(&mut svg, name, resources, doc, &state);
                        }
                    }

                    _ => {
                        // Unhandled operator — skip
                    }
                }

                operands.clear();
            }
        }
    }

    svg
}

fn emit_path(svg: &mut String, path_data: &str, state: &GraphicsState, fill: bool, stroke: bool) {
    let fill_attr = if fill {
        format!(r#"fill="{}""#, state.fill_color.to_css())
    } else {
        r#"fill="none""#.to_string()
    };

    let stroke_attr = if stroke {
        format!(
            r#"stroke="{}" stroke-width="{}""#,
            state.stroke_color.to_css(),
            state.line_width
        )
    } else {
        r#"stroke="none""#.to_string()
    };

    let linecap = match state.line_cap {
        0 => "butt",
        1 => "round",
        2 => "square",
        _ => "butt",
    };

    let linejoin = match state.line_join {
        0 => "miter",
        1 => "round",
        2 => "bevel",
        _ => "miter",
    };

    let _ = write!(
        svg,
        r#"<path d="{}" {} {} stroke-linecap="{linecap}" stroke-linejoin="{linejoin}" transform="matrix({},{},{},{},{},{})"/>"#,
        path_data.trim(),
        fill_attr,
        stroke_attr,
        state.ctm[0], state.ctm[1], state.ctm[2], state.ctm[3],
        state.ctm[4], state.ctm[5],
    );
    svg.push('\n');
}

fn emit_text(svg: &mut String, text: &str, state: &GraphicsState) {
    let tm = multiply_matrices(&state.text_matrix, &state.ctm);
    let x = tm[4];
    let y = tm[5];
    let font_size = state.font_size * tm[3].abs().max(tm[0].abs());

    // Use a generic font family based on the PDF font name
    let font_family = if state.font_name.contains("Courier") || state.font_name.contains("Mono") {
        "monospace"
    } else if state.font_name.contains("Times") || state.font_name.contains("Serif") {
        "serif"
    } else {
        "sans-serif"
    };

    let escaped = escape_xml(text);

    // Note: PDF Y grows up, SVG Y grows down. The parent <g> has scale(1,-1),
    // so we need to flip text back with scale(1,-1) to make it readable.
    let _ = write!(
        svg,
        r#"<text x="{x}" y="{y}" font-family="{font_family}" font-size="{font_size:.1}" fill="{}" transform="scale(1,-1) translate(0,{})">{escaped}</text>"#,
        state.fill_color.to_css(),
        -2.0 * y,
    );
    svg.push('\n');
}

fn emit_xobject(
    svg: &mut String,
    name: &[u8],
    resources: &Option<&lopdf::Dictionary>,
    doc: &Document,
    state: &GraphicsState,
) {
    let resources = match resources {
        Some(r) => r,
        None => return,
    };

    let xobjects = match resources.get(b"XObject").ok().and_then(|o| {
        match o {
            Object::Reference(id) => doc.get_object(*id).ok(),
            _ => Some(o),
        }
        .and_then(|o| o.as_dict().ok())
    }) {
        Some(x) => x,
        None => return,
    };

    let xobj = match xobjects.get(name).ok().and_then(|o| match o {
        Object::Reference(id) => doc.get_object(*id).ok(),
        _ => Some(o),
    }) {
        Some(o) => o,
        None => return,
    };

    if let Object::Stream(stream) = xobj {
        let subtype = stream
            .dict
            .get(b"Subtype")
            .ok()
            .and_then(|o| o.as_name().ok())
            .map(|n| n.to_vec());

        if subtype.as_deref() == Some(b"Image") {
            // Extract image and embed as base64
            let width = stream
                .dict
                .get(b"Width")
                .ok()
                .and_then(|o| o.as_i64().ok())
                .unwrap_or(0);
            let height = stream
                .dict
                .get(b"Height")
                .ok()
                .and_then(|o| o.as_i64().ok())
                .unwrap_or(0);

            let filter = stream
                .dict
                .get(b"Filter")
                .ok()
                .and_then(|o| o.as_name().ok())
                .map(|n| n.to_vec());

            if filter.as_deref() == Some(b"DCTDecode") {
                // JPEG — embed directly
                let data = &stream.content;
                let b64 = base64::Engine::encode(
                    &base64::engine::general_purpose::STANDARD,
                    data,
                );
                let _ = write!(
                    svg,
                    r#"<image x="0" y="0" width="{width}" height="{height}" href="data:image/jpeg;base64,{b64}" transform="matrix({},{},{},{},{},{})"/>"#,
                    state.ctm[0], state.ctm[1], state.ctm[2], state.ctm[3],
                    state.ctm[4], state.ctm[5],
                );
                svg.push('\n');
            }
        }
    }
}

// Simple PDF content stream tokenizer
#[derive(Debug)]
enum Token {
    Operand(Object),
    Operator(String),
}

fn tokenize_content_stream(data: &[u8]) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut i = 0;
    let len = data.len();

    while i < len {
        // Skip whitespace
        while i < len && (data[i] == b' ' || data[i] == b'\n' || data[i] == b'\r' || data[i] == b'\t') {
            i += 1;
        }
        if i >= len {
            break;
        }

        match data[i] {
            // Comment
            b'%' => {
                while i < len && data[i] != b'\n' && data[i] != b'\r' {
                    i += 1;
                }
            }
            // String literal
            b'(' => {
                let mut depth = 1;
                let mut s = Vec::new();
                i += 1;
                while i < len && depth > 0 {
                    match data[i] {
                        b'(' => {
                            depth += 1;
                            s.push(data[i]);
                        }
                        b')' => {
                            depth -= 1;
                            if depth > 0 {
                                s.push(data[i]);
                            }
                        }
                        b'\\' if i + 1 < len => {
                            i += 1;
                            match data[i] {
                                b'n' => s.push(b'\n'),
                                b'r' => s.push(b'\r'),
                                b't' => s.push(b'\t'),
                                b'(' => s.push(b'('),
                                b')' => s.push(b')'),
                                b'\\' => s.push(b'\\'),
                                _ => s.push(data[i]),
                            }
                        }
                        _ => s.push(data[i]),
                    }
                    i += 1;
                }
                tokens.push(Token::Operand(Object::String(
                    s,
                    lopdf::StringFormat::Literal,
                )));
            }
            // Hex string
            b'<' if i + 1 < len && data[i + 1] == b'<' => {
                // Dictionary start — skip (inline dicts in content streams are rare)
                i += 2;
            }
            b'<' => {
                i += 1;
                let mut hex = Vec::new();
                while i < len && data[i] != b'>' {
                    if data[i].is_ascii_hexdigit() {
                        hex.push(data[i]);
                    }
                    i += 1;
                }
                if i < len {
                    i += 1;
                }
                // Decode hex pairs
                let mut bytes = Vec::new();
                let mut j = 0;
                while j + 1 < hex.len() {
                    let high = char::from(hex[j]).to_digit(16).unwrap_or(0) as u8;
                    let low = char::from(hex[j + 1]).to_digit(16).unwrap_or(0) as u8;
                    bytes.push((high << 4) | low);
                    j += 2;
                }
                tokens.push(Token::Operand(Object::String(
                    bytes,
                    lopdf::StringFormat::Hexadecimal,
                )));
            }
            // Name
            b'/' => {
                i += 1;
                let start = i;
                while i < len
                    && data[i] != b' '
                    && data[i] != b'\n'
                    && data[i] != b'\r'
                    && data[i] != b'\t'
                    && data[i] != b'/'
                    && data[i] != b'('
                    && data[i] != b'<'
                    && data[i] != b'['
                    && data[i] != b']'
                {
                    i += 1;
                }
                tokens.push(Token::Operand(Object::Name(data[start..i].to_vec())));
            }
            // Array
            b'[' => {
                i += 1;
                let mut arr = Vec::new();
                let mut depth = 1;
                // Simple: collect tokens until matching ]
                let inner_start = i;
                while i < len && depth > 0 {
                    if data[i] == b'[' {
                        depth += 1;
                    } else if data[i] == b']' {
                        depth -= 1;
                    }
                    if depth > 0 {
                        i += 1;
                    }
                }
                // Parse inner tokens
                let inner = &data[inner_start..i];
                let inner_tokens = tokenize_content_stream(inner);
                for t in inner_tokens {
                    if let Token::Operand(obj) = t {
                        arr.push(obj);
                    }
                }
                if i < len {
                    i += 1;
                }
                tokens.push(Token::Operand(Object::Array(arr)));
            }
            b']' | b'>' => {
                i += 1;
            }
            // Number or operator
            _ => {
                let start = i;
                while i < len
                    && data[i] != b' '
                    && data[i] != b'\n'
                    && data[i] != b'\r'
                    && data[i] != b'\t'
                    && data[i] != b'/'
                    && data[i] != b'('
                    && data[i] != b'<'
                    && data[i] != b'['
                    && data[i] != b']'
                {
                    i += 1;
                }
                let word = &data[start..i];
                let word_str = String::from_utf8_lossy(word);

                // Try to parse as number
                if let Ok(n) = word_str.parse::<i64>() {
                    tokens.push(Token::Operand(Object::Integer(n)));
                } else if let Ok(f) = word_str.parse::<f64>() {
                    tokens.push(Token::Operand(Object::Real(f as f32)));
                } else if word_str == "true" {
                    tokens.push(Token::Operand(Object::Boolean(true)));
                } else if word_str == "false" {
                    tokens.push(Token::Operand(Object::Boolean(false)));
                } else if word_str == "null" {
                    tokens.push(Token::Operand(Object::Null));
                } else {
                    tokens.push(Token::Operator(word_str.to_string()));
                }
            }
        }
    }

    tokens
}
