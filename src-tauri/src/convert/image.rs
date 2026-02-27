use std::path::Path;

use super::suffix_path;

pub fn convert_grayscale(input: &Path) -> Result<Vec<String>, String> {
    let img = image::open(input).map_err(|e| e.to_string())?;
    let gray = img.grayscale();
    let output = suffix_path(input, "_gray", None);
    gray.save(&output).map_err(|e| e.to_string())?;
    Ok(vec![output.to_string_lossy().to_string()])
}

pub fn image_to_pdf(input: &Path) -> Result<Vec<String>, String> {
    use printpdf::*;

    let img_data = std::fs::read(input).map_err(|e| e.to_string())?;
    let dyn_img = ::image::load_from_memory(&img_data).map_err(|e| e.to_string())?;

    let (w, h) = (dyn_img.width(), dyn_img.height());
    let rgb = dyn_img.to_rgb8();

    // 72 DPI: pixels map 1:1 to points
    let page_w = Mm(w as f32 * 25.4 / 72.0);
    let page_h = Mm(h as f32 * 25.4 / 72.0);

    let (doc, page1, layer1) = PdfDocument::new("Image", page_w, page_h, "Layer 1");
    let current_layer = doc.get_page(page1).get_layer(layer1);

    let pdf_image = Image::from(ImageXObject {
        width: Px(w as usize),
        height: Px(h as usize),
        color_space: ColorSpace::Rgb,
        bits_per_component: ColorBits::Bit8,
        interpolate: true,
        image_data: rgb.into_raw(),
        image_filter: None,
        clipping_bbox: None,
        smask: None,
    });

    pdf_image.add_to_layer(
        current_layer,
        ImageTransform {
            translate_x: Some(Mm(0.0)),
            translate_y: Some(Mm(0.0)),
            scale_x: Some(page_w.into_pt().0 / w as f32),
            scale_y: Some(page_h.into_pt().0 / h as f32),
            ..Default::default()
        },
    );

    let output = suffix_path(input, "", Some("pdf"));
    let file = std::fs::File::create(&output).map_err(|e| e.to_string())?;
    doc.save(&mut std::io::BufWriter::new(file))
        .map_err(|e| e.to_string())?;

    Ok(vec![output.to_string_lossy().to_string()])
}
