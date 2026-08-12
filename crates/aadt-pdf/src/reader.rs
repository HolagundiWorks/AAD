//! Open / rasterize PDF pages (ADR-0019).
//!
//! Built always: types + graceful “unavailable” errors.
//! Real rasterization requires Cargo feature `pdfium` and a runtime `pdfium`
//! dynamic library beside the executable (or on PATH).
//!
//! Documents are re-opened per render so AADT stays `unsafe_code = forbid`
//! (no Pdfium lifetime tricks in this crate).

use std::path::{Path, PathBuf};

/// Why a PDF could not be opened or rendered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PdfError {
    /// Feature `pdfium` not enabled in this build.
    FeatureDisabled,
    /// Native Pdfium library missing or failed to bind.
    Library(String),
    /// File I/O or Pdfium document error.
    Document(String),
    /// Page index out of range.
    BadPage(usize),
}

impl std::fmt::Display for PdfError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PdfError::FeatureDisabled => write!(
                f,
                "PDF open needs the `pdfium` feature (rebuild aadt-app with gui)"
            ),
            PdfError::Library(m) => write!(f, "Pdfium library: {m}"),
            PdfError::Document(m) => write!(f, "PDF document: {m}"),
            PdfError::BadPage(i) => write!(f, "PDF page {i} out of range"),
        }
    }
}

impl std::error::Error for PdfError {}

/// One rasterized page (RGBA8, top-left origin for egui textures).
#[derive(Debug, Clone)]
pub struct RenderedPage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// PDF page width in points.
    pub page_width_pt: f32,
    /// PDF page height in points.
    pub page_height_pt: f32,
}

/// Lightweight handle: path + page count (re-opens for each render).
#[derive(Debug, Clone)]
pub struct OpenedPdf {
    pub path: PathBuf,
    pub page_count: usize,
}

/// Try to bind Pdfium and read page count from `path`.
pub fn open_pdf(path: impl AsRef<Path>) -> Result<OpenedPdf, PdfError> {
    #[cfg(not(feature = "pdfium"))]
    {
        let _ = path;
        Err(PdfError::FeatureDisabled)
    }
    #[cfg(feature = "pdfium")]
    {
        open_pdf_pdfium(path.as_ref())
    }
}

/// Rasterize `page_index` (0-based) at approximately `target_width` CSS pixels wide.
pub fn render_page(
    doc: &OpenedPdf,
    page_index: usize,
    target_width: u32,
) -> Result<RenderedPage, PdfError> {
    #[cfg(not(feature = "pdfium"))]
    {
        let _ = (doc, page_index, target_width);
        Err(PdfError::FeatureDisabled)
    }
    #[cfg(feature = "pdfium")]
    {
        render_page_pdfium(doc, page_index, target_width.max(64))
    }
}

/// Whether this build was compiled with the `pdfium` feature.
pub fn pdfium_feature_enabled() -> bool {
    cfg!(feature = "pdfium")
}

#[cfg(feature = "pdfium")]
fn bind_pdfium() -> Result<pdfium_render::prelude::Pdfium, PdfError> {
    use pdfium_render::prelude::*;
    let bindings = Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path("./"))
        .or_else(|_| Pdfium::bind_to_system_library())
        .map_err(|e| PdfError::Library(e.to_string()))?;
    Ok(Pdfium::new(bindings))
}

#[cfg(feature = "pdfium")]
fn open_pdf_pdfium(path: &Path) -> Result<OpenedPdf, PdfError> {
    let pdfium = bind_pdfium()?;
    let document = pdfium
        .load_pdf_from_file(path, None)
        .map_err(|e| PdfError::Document(e.to_string()))?;
    let page_count = document.pages().len() as usize;
    Ok(OpenedPdf {
        path: path.to_path_buf(),
        page_count,
    })
}

#[cfg(feature = "pdfium")]
fn render_page_pdfium(
    doc: &OpenedPdf,
    page_index: usize,
    target_width: u32,
) -> Result<RenderedPage, PdfError> {
    use pdfium_render::prelude::*;

    if page_index >= doc.page_count {
        return Err(PdfError::BadPage(page_index));
    }
    let pdfium = bind_pdfium()?;
    let document = pdfium
        .load_pdf_from_file(&doc.path, None)
        .map_err(|e| PdfError::Document(e.to_string()))?;
    let page = document
        .pages()
        .get(page_index as u16)
        .map_err(|e| PdfError::Document(e.to_string()))?;
    let page_width_pt = page.width().value;
    let page_height_pt = page.height().value;
    let scale = target_width as f32 / page_width_pt.max(1.0);
    let render_config = PdfRenderConfig::new()
        .set_target_width(target_width as i32)
        .set_maximum_height((page_height_pt * scale).ceil() as i32);
    let image = page
        .render_with_config(&render_config)
        .map_err(|e| PdfError::Document(e.to_string()))?
        .as_image();
    let rgba = image.to_rgba8();
    Ok(RenderedPage {
        width: rgba.width(),
        height: rgba.height(),
        rgba: rgba.into_raw(),
        page_width_pt,
        page_height_pt,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_flag_is_consistent() {
        let enabled = pdfium_feature_enabled();
        let err = open_pdf("nope.pdf");
        if !enabled {
            assert_eq!(err.unwrap_err(), PdfError::FeatureDisabled);
        }
    }
}
