use std::path::Path;

use pdfium_bind::PdfDocument;

pub fn to_images(input: &Path, format: &str, dpi: u32) -> Result<Vec<String>, String> {
    let doc = PdfDocument::open(input)?;

    let output_dir = input.parent().unwrap_or_else(|| Path::new("."));
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");

    let ext = match format {
        "jpeg" | "jpg" => "jpg",
        "png" => "png",
        "tiff" | "tif" => "tiff",
        "ppm" => "ppm",
        _ => return Err(format!("Unsupported image format: {format}")),
    };

    let page_count = doc.page_count();
    if page_count == 0 {
        return Err("PDF has no pages".to_string());
    }

    let mut outputs = Vec::new();

    for page_idx in 0..page_count {
        let (rgba_data, width, height) =
            doc.render_page(page_idx, dpi as f32)?;

        // Create an RGBA image from the pixel data
        let img: image::RgbaImage =
            image::ImageBuffer::from_raw(width as u32, height as u32, rgba_data)
                .ok_or_else(|| "Failed to create image from rendered page".to_string())?;

        let page_num = page_idx + 1;
        let output_path = output_dir.join(format!("{stem}-{page_num:03}.{ext}"));

        let img_format = match format {
            "jpeg" | "jpg" => {
                // JPEG doesn't support alpha, convert to RGB
                let rgb: image::RgbImage = image::DynamicImage::ImageRgba8(img).to_rgb8();
                rgb.save_with_format(&output_path, image::ImageFormat::Jpeg)
                    .map_err(|e| format!("Failed to save JPEG: {e}"))?;
                outputs.push(output_path.to_string_lossy().to_string());
                continue;
            }
            "png" => image::ImageFormat::Png,
            "tiff" | "tif" => image::ImageFormat::Tiff,
            "ppm" => {
                // PPM doesn't support alpha, convert to RGB
                let rgb: image::RgbImage = image::DynamicImage::ImageRgba8(img).to_rgb8();
                rgb.save_with_format(&output_path, image::ImageFormat::Pnm)
                    .map_err(|e| format!("Failed to save PPM: {e}"))?;
                outputs.push(output_path.to_string_lossy().to_string());
                continue;
            }
            _ => unreachable!(),
        };

        img.save_with_format(&output_path, img_format)
            .map_err(|e| format!("Failed to save {ext}: {e}"))?;

        outputs.push(output_path.to_string_lossy().to_string());
    }

    Ok(outputs)
}
