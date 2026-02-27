pub mod image;
pub mod pdf_extract;
pub mod pdf_gs;
pub mod pdf_images;
pub mod pdf_meta;
pub mod pdf_pdfa;
pub mod pdf_split;
pub mod pdf_svg;
pub mod pdf_text;

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConvertRequest {
    pub input_path: String,
    pub conversion_type: String,
    #[serde(default)]
    pub input_file: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConvertResponse {
    pub output_paths: Vec<String>,
}

pub fn suffix_path(input: &Path, suffix: &str, new_ext: Option<&str>) -> PathBuf {
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

pub fn dispatch(req: ConvertRequest) -> Result<ConvertResponse, String> {
    let input = PathBuf::from(&req.input_path);
    if !input.exists() {
        return Err("Input path does not exist".to_string());
    }

    let output_paths = match req.conversion_type.as_str() {
        // Image operations
        "grayscalepic" => image::convert_grayscale(&input)?,
        "image2pdf" => image::image_to_pdf(&input)?,

        // PDF text extraction
        "pdf2txt" => pdf_text::extract_text(&input)?,

        // PDF metadata
        "pdfmetaremove" => pdf_meta::remove_metadata(&input)?,
        "pdfmetaclone" => {
            let source = req
                .input_file
                .ok_or_else(|| "inputFile is required for pdfmetaclone".to_string())?;
            pdf_meta::clone_metadata(Path::new(&source), &input)?
        }

        // PDF split/join
        "pdf2split" => pdf_split::split_pages(&input)?,
        "pdf2join" => pdf_split::join_pdfs(&input)?,

        // PDF to raster images (poppler)
        "pdf2jpeglow" => pdf_images::to_images(&input, "jpeg", 250)?,
        "pdf2jpegmedium" => pdf_images::to_images(&input, "jpeg", 500)?,
        "pdf2jpeghigh" => pdf_images::to_images(&input, "jpeg", 1000)?,
        "pdf2pnglow" => pdf_images::to_images(&input, "png", 250)?,
        "pdf2pngmedium" => pdf_images::to_images(&input, "png", 500)?,
        "pdf2pnghigh" => pdf_images::to_images(&input, "png", 1000)?,
        "pdf2tifflow" => pdf_images::to_images(&input, "tiff", 250)?,
        "pdf2tiffmedium" => pdf_images::to_images(&input, "tiff", 500)?,
        "pdf2tiffhigh" => pdf_images::to_images(&input, "tiff", 1000)?,
        "pdf2ppmlow" => pdf_images::to_images(&input, "ppm", 250)?,
        "pdf2ppmmedium" => pdf_images::to_images(&input, "ppm", 500)?,
        "pdf2ppmhigh" => pdf_images::to_images(&input, "ppm", 1000)?,

        // PDF to SVG (native)
        "pdf2svg" => pdf_svg::to_svg(&input)?,

        // PDF extract embedded images/attachments (poppler)
        "pdf2extract" => pdf_extract::extract_images(&input)?,
        "pdf2detach" => pdf_extract::detach_files(&input)?,

        // PDF compression (ghostscript)
        "pdf2pdflow" => pdf_gs::compress(&input, "screen")?,
        "pdf2pdfmedium" => pdf_gs::compress(&input, "ebook")?,
        "pdf2pdfhigh" => pdf_gs::compress(&input, "printer")?,

        // PDF/A conversion (native) — all conformance levels
        "pdf2pdfa" | "pdf2pdfa2b" => {
            pdf_pdfa::to_pdfa(&input, pdf_pdfa::PdfaLevel::A2b, None)?
        }
        "pdf2pdfa1b" => pdf_pdfa::to_pdfa(&input, pdf_pdfa::PdfaLevel::A1b, None)?,
        "pdf2pdfa1a" => pdf_pdfa::to_pdfa(&input, pdf_pdfa::PdfaLevel::A1a, None)?,
        "pdf2pdfa2a" => pdf_pdfa::to_pdfa(&input, pdf_pdfa::PdfaLevel::A2a, None)?,
        "pdf2pdfa2u" => pdf_pdfa::to_pdfa(&input, pdf_pdfa::PdfaLevel::A2u, None)?,
        "pdf2pdfa3" => pdf_pdfa::to_pdfa(&input, pdf_pdfa::PdfaLevel::A3, None)?,
        "pdf2pdfa4" => pdf_pdfa::to_pdfa(&input, pdf_pdfa::PdfaLevel::A4, None)?,
        "pdf2pdfa4f" => pdf_pdfa::to_pdfa(&input, pdf_pdfa::PdfaLevel::A4f, None)?,
        "pdf2pdfa4e" => pdf_pdfa::to_pdfa(&input, pdf_pdfa::PdfaLevel::A4e, None)?,

        // PDF/X conversion (native) — print production profiles
        "pdf2pdfx1a" => pdf_pdfa::to_pdfx(&input, pdf_pdfa::PdfxLevel::X1a)?,
        "pdf2pdfx3" => pdf_pdfa::to_pdfx(&input, pdf_pdfa::PdfxLevel::X3)?,
        "pdf2pdfx4" => pdf_pdfa::to_pdfx(&input, pdf_pdfa::PdfxLevel::X4)?,

        // PDF stamping (ghostscript)
        "pdfstamp1" => {
            let stamp = req
                .input_file
                .ok_or_else(|| "inputFile (stamp PDF) is required for pdfstamp1".to_string())?;
            pdf_gs::stamp(&input, Path::new(&stamp))?
        }

        // Document to PDF (LibreOffice)
        "doc2pdf" => pdf_gs::doc_to_pdf(&input)?,

        other => {
            return Err(format!("Conversion type '{other}' is not supported."));
        }
    };

    Ok(ConvertResponse { output_paths })
}
