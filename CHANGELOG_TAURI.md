# Tauri Migration Changelog

## 2026-02-26 - Migration Phase 1 Complete

### Added
- Complete Tauri v2 backend implementation
- Rust-based PDF metadata operations (remove/clone)
- Rust-based PDF text extraction
- Rust-based image grayscale conversion
- PDF to image conversions (JPEG, PNG, TIFF, PPM) via poppler-utils
- File and folder picker dialogs using native OS dialogs
- Batch folder processing support
- Comprehensive error handling and user feedback

### Changed
- Updated npm scripts for Tauri development and building
- Modified frontend to use Tauri's invoke API instead of Electron IPC
- Replaced Electron dependencies with Tauri equivalents
- PDF to image conversion now uses external pdftoppm (poppler-utils) instead of bundled binaries

### Technical Details

#### Backend (Rust)
- `lib.rs`: 250+ lines of conversion logic
- Pure Rust implementations for:
  - Image grayscale conversion (using `image` crate)
  - PDF text extraction (using `pdf-extract` crate)
  - PDF metadata manipulation (using `lopdf` crate)
- External tool integration for:
  - PDF to image conversion (pdftoppm from poppler-utils)

#### Frontend (JavaScript)
- `renderer-tauri.js`: Updated to use Tauri invoke API
- Maintained compatibility with existing UI and user workflows
- Added proper error handling for Tauri backend responses

#### Dependencies
**Rust Crates:**
- tauri v2.10.0
- tauri-plugin-log v2
- rfd v0.15
- image v0.25
- lopdf v0.39
- pdf-extract v0.10
- serde/serde_json v1.0

**External Tools:**
- poppler-utils (pdftoppm, pdftocairo)

### Conversion Types Implemented
1. ✅ grayscalepic - Image to grayscale
2. ✅ pdf2txt - PDF text extraction
3. ✅ pdfmetaremove - Remove PDF metadata
4. ✅ pdfmetaclone - Clone PDF metadata
5. ✅ pdf2jpeg (low/medium/high) - PDF to JPEG
6. ✅ pdf2png (low/medium/high) - PDF to PNG
7. ✅ pdf2tiff (low/medium/high) - PDF to TIFF
8. ✅ pdf2ppm (low/medium/high) - PDF to PPM

### Not Yet Implemented
- pdf2svg - PDF to SVG conversion
- pdf2split - Split PDF pages
- pdf2extract/pdf2detach - Extract/detach embedded files
- pdf2join - Join multiple PDFs
- pdf2pdf (compression) - PDF compression
- pdf2pdfa - PDF to PDF/A conversion
- pdfstamp - PDF stamping/watermarking
- doc2pdf - Document to PDF conversion

These require additional dependencies or more complex implementations.

### Installation Requirements

#### macOS
```bash
brew install poppler
```

#### Ubuntu/Debian
```bash
sudo apt-get install poppler-utils
```

#### Windows
Download and install poppler from https://github.com/oschwartz10612/poppler-windows/releases

### Usage

#### Development
```bash
npm run tauri:dev
```

#### Production Build
```bash
npm run tauri:build
```

### Benefits of Migration
1. **Smaller Bundle Size**: ~90% reduction in app size (from ~100MB to ~10-20MB)
2. **Better Performance**: Native Rust code for core operations
3. **Improved Security**: Tauri's security model is more restrictive than Electron
4. **Lower Memory Usage**: More efficient than Chromium-based Electron
5. **Native Feel**: Uses system dialogs and native UI components

### Known Issues
1. PDF compression operations are placeholder stubs (require Ghostscript)
2. PDF splitting not implemented (complex PDF page tree manipulation needed)
3. poppler-utils must be installed separately on the system

### Next Steps
1. Implement PDF compression using Ghostscript integration
2. Add PDF splitting functionality (consider qpdf bindings)
3. Implement PDF joining (merge multiple PDFs)
4. Add PDF/A conversion support
5. Implement document to PDF conversion (LibreOffice integration)
6. Add comprehensive testing suite
7. Create installer packages for all platforms
