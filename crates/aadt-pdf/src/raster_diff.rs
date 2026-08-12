//! Raster page comparison — deterministic pixel diff for sheet / PDF revisions.
//!
//! Does not require PDFium: callers pass RGBA8 buffers (e.g. from
//! [`crate::reader`] or synthetic tests).

/// Result of comparing two equal-sized RGBA images.
#[derive(Debug, Clone, PartialEq)]
pub struct RasterDiff {
    pub width: u32,
    pub height: u32,
    /// Pixels that differ beyond `tolerance` (any channel).
    pub changed_pixels: u64,
    pub total_pixels: u64,
    /// Fraction in \[0, 1\].
    pub changed_ratio: f64,
}

impl RasterDiff {
    pub fn is_identical(&self) -> bool {
        self.changed_pixels == 0
    }

    pub fn narrate(&self) -> String {
        if self.is_identical() {
            return format!(
                "Raster compare: identical ({}×{}).",
                self.width, self.height
            );
        }
        format!(
            "Raster compare: {:.2}% pixels differ ({} of {} on {}×{}).",
            self.changed_ratio * 100.0,
            self.changed_pixels,
            self.total_pixels,
            self.width,
            self.height
        )
    }
}

/// Compare two RGBA8 buffers. Lengths must be `width * height * 4`.
/// `tolerance` is per-channel absolute delta (0 = exact).
pub fn compare_rgba(
    a: &[u8],
    b: &[u8],
    width: u32,
    height: u32,
    tolerance: u8,
) -> Result<RasterDiff, String> {
    let need = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(4))
        .ok_or_else(|| "raster size overflow".to_string())?;
    if a.len() != need || b.len() != need {
        return Err(format!(
            "RGBA length mismatch: expected {need}, got a={} b={}",
            a.len(),
            b.len()
        ));
    }
    let total = (width as u64) * (height as u64);
    let mut changed = 0u64;
    for i in 0..total as usize {
        let o = i * 4;
        let diff = (0..4).any(|c| a[o + c].abs_diff(b[o + c]) > tolerance);
        if diff {
            changed += 1;
        }
    }
    let ratio = if total == 0 {
        0.0
    } else {
        changed as f64 / total as f64
    };
    Ok(RasterDiff {
        width,
        height,
        changed_pixels: changed,
        total_pixels: total,
        changed_ratio: ratio,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_buffers() {
        let px = vec![10u8, 20, 30, 255, 1, 2, 3, 255];
        let d = compare_rgba(&px, &px, 2, 1, 0).unwrap();
        assert!(d.is_identical());
        assert!(d.narrate().contains("identical"));
    }

    #[test]
    fn detects_changed_pixel() {
        let a = vec![0u8, 0, 0, 255, 0, 0, 0, 255];
        let mut b = a.clone();
        b[0] = 255;
        let d = compare_rgba(&a, &b, 2, 1, 0).unwrap();
        assert_eq!(d.changed_pixels, 1);
        assert!((d.changed_ratio - 0.5).abs() < 1e-9);
    }

    #[test]
    fn tolerance_ignores_small_noise() {
        let a = vec![100u8, 100, 100, 255];
        let b = vec![102u8, 100, 100, 255];
        let d = compare_rgba(&a, &b, 1, 1, 3).unwrap();
        assert!(d.is_identical());
    }
}
