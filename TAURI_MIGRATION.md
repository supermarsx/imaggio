# Tauri Migration Progress

## Overview
This document tracks the migration from Electron to Tauri for the Imaggio PDF manipulation tool.

## Completed Features

### Core Infrastructure ✅
- [x] Tauri project setup (v2.10.0)
- [x] Basic window configuration
- [x] File and folder picker dialogs
- [x] Directory listing functionality
- [x] Frontend updated to use Tauri invoke API
- [x] Build configuration in Cargo.toml and tauri.conf.json

### Implemented Conversions ✅

#### Image Processing
- [x] **grayscalepic** - Convert images to grayscale using `image` crate

#### PDF Operations
- [x] **pdf2txt** - Extract text from PDF using `pdf-extract` crate
- [x] **pdfmetaremove** - Remove PDF metadata using `lopdf` crate
- [x] **pdfmetaclone** - Clone metadata from one PDF to another using `lopdf` crate

#### PDF to Image Conversions (using external `poppler-utils`)
- [x] **pdf2jpeglow** - PDF to JPEG (2000px width) via pdftoppm
- [x] **pdf2jpegmedium** - PDF to JPEG (4000px width) via pdftoppm
- [x] **pdf2jpeghigh** - PDF to JPEG (8000px width) via pdftoppm
- [x] **pdf2pnglow** - PDF to PNG (2000px width) via pdftoppm
- [x] **pdf2pngmedium** - PDF to PNG (4000px width) via pdftoppm
- [x] **pdf2pnghigh** - PDF to PNG (8000px width) via pdftoppm
- [x] **pdf2tifflow** - PDF to TIFF (2000px width) via pdftoppm
- [x] **pdf2tiffmedium** - PDF to TIFF (4000px width) via pdftoppm
- [x] **pdf2tiffhigh** - PDF to TIFF (8000px width) via pdftoppm
- [x] **pdf2ppmlow** - PDF to PPM (2000px width) via pdftoppm
- [x] **pdf2ppmmedium** - PDF to PPM (4000px width) via pdftoppm
- [x] **pdf2ppmhigh** - PDF to PPM (8000px width) via pdftoppm

## Not Yet Implemented ⚠️

### Complex PDF Operations (require additional dependencies or external tools)
- [ ] **pdf2svg** - PDF to SVG (needs svg rendering library)
- [ ] **pdf2split** - Split PDF into individual pages (needs better PDF manipulation)
- [ ] **pdf2extract** - Extract embedded files (needs PDF stream handling)
- [ ] **pdf2detach** - Detach embedded files (needs PDF stream handling)
- [ ] **pdf2join** - Join multiple PDFs (needs PDF merging capability)
- [ ] **pdf2pdflow/medium/high** - PDF compression (requires Ghostscript or similar)
- [ ] **pdf2pdfa/pdfalow/pdfamedium/pdfahigh** - PDF to PDF/A conversion (requires Ghostscript)
- [ ] **pdfstamp1** - PDF stamping/watermarking (needs PDF content stream manipulation)
- [ ] **doc2pdf** - Document to PDF conversion (requires LibreOffice or similar)

## Dependencies

### Rust Crates
- `tauri` v2.10.0 - Main framework
- `tauri-plugin-log` v2 - Logging
- `rfd` v0.15 - File dialogs
- `image` v0.25 - Image processing
- `lopdf` v0.39 - PDF manipulation
- `pdf-extract` v0.10 - PDF text extraction
- `serde` v1.0 - Serialization
- `serde_json` v1.0 - JSON handling

### External System Dependencies
The following external tools are called via `std::process::Command`:
- **poppler-utils** (pdftoppm) - PDF to image conversion
  - Install on macOS: `brew install poppler`
  - Install on Ubuntu/Debian: `apt-get install poppler-utils`
  - Install on Windows: Download from poppler releases

### External Dependencies for Future Implementation
Some conversions may require external tools:
- **Ghostscript** - PDF compression and PDF/A conversion
- **pdftk** or **qpdf** - Advanced PDF splitting/merging
- **LibreOffice** - Document to PDF conversion
- **poppler-utils** - Alternative for some PDF operations

## Running the Application

### Development Mode
```bash
npm run tauri:dev
```

### Build for Production
```bash
npm run tauri:build
```

### Legacy Electron Mode
```bash
npm start
```

## Migration Strategy

### Phase 1: Core Features (COMPLETED)
- Basic app structure and dialogs
- Simple image conversions
- PDF metadata operations
- PDF to image conversions

### Phase 2: Advanced PDF Operations (PENDING)
- PDF splitting and merging
- PDF compression
- PDF/A conversion
- Attachment handling

### Phase 3: Document Conversion (PENDING)
- Office document to PDF
- PDF stamping/watermarking

## Notes

1. **Poppler Dependency**: PDF to image conversions use the external `pdftoppm` tool from poppler-utils. This must be installed separately on the system.

2. **Performance**: Pure Rust implementations (grayscale, metadata operations, text extraction) are generally faster than calling external binaries

3. **File Size**: Tauri apps are significantly smaller than Electron apps (~10-20MB vs ~100MB+)

4. **Platform Support**: Current configuration targets all platforms (Linux, macOS, Windows)

## Testing Checklist

- [ ] Test file picker dialog
- [ ] Test folder picker dialog
- [ ] Test grayscale image conversion
- [ ] Test PDF text extraction
- [ ] Test PDF metadata removal
- [ ] Test PDF metadata cloning
- [ ] Test PDF to JPEG conversion (all quality levels)
- [ ] Test PDF to PNG conversion (all quality levels)
- [ ] Test folder batch processing
- [ ] Test error handling
- [ ] Test UI state management

## Known Issues

1. PDF compression is currently a stub (copies file) - needs Ghostscript integration
2. PDF splitting not implemented - requires more complex PDF manipulation
3. Some conversions from original Electron app require external tools

## Future Improvements

1. Integrate Ghostscript for PDF compression and PDF/A
2. Add proper PDF page manipulation for splitting
3. Consider using `qpdf` Rust bindings if they become available
4. Add progress bars for long operations
5. Implement drag-and-drop file handling
6. Add batch processing progress indicators
7. Implement PDF joining functionality
