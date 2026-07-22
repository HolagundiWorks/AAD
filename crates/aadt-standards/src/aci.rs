//! AutoCAD Color Index (ACI) → sRGB and WCAG contrast against white paper.

/// Relative luminance (WCAG 2.x) for an 8-bit sRGB colour.
pub fn relative_luminance(r: u8, g: u8, b: u8) -> f64 {
    fn channel(c: u8) -> f64 {
        let s = f64::from(c) / 255.0;
        if s <= 0.04045 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
}

/// Contrast ratio between two relative luminances (WCAG).
pub fn contrast_ratio(l1: f64, l2: f64) -> f64 {
    let (hi, lo) = if l1 >= l2 { (l1, l2) } else { (l2, l1) };
    (hi + 0.05) / (lo + 0.05)
}

/// Contrast of an sRGB colour against white paper (#FFFFFF).
pub fn contrast_vs_white(r: u8, g: u8, b: u8) -> f64 {
    contrast_ratio(relative_luminance(r, g, b), 1.0)
}

/// WCAG 2.1 AA minimum contrast for non-text graphical objects.
pub const GRAPHICS_AA_MIN: f64 = 3.0;

/// Map ACI 1–255 to sRGB for screen / print preview.
///
/// ACI 7 (white/black) is mapped to black so it remains visible on a white
/// paper canvas. ACI 0 (ByBlock) is treated as black.
pub fn aci_to_srgb(aci: u8) -> (u8, u8, u8) {
    match aci {
        0 | 7 => (0, 0, 0),
        1 => (255, 0, 0),
        2 => (255, 255, 0),
        3 => (0, 255, 0),
        4 => (0, 255, 255),
        5 => (0, 0, 255),
        6 => (255, 0, 255),
        8 => (128, 128, 128),
        9 => (192, 192, 192),
        250 => (0, 0, 0),
        251 => (51, 51, 51),
        252 => (102, 102, 102),
        253 => (153, 153, 153),
        254 => (204, 204, 204),
        255 => (255, 255, 255),
        n => {
            // Autodesk ACI 10–249: 24 hues × 5 values × 2 saturations (approx.).
            let idx = (n - 10) as usize;
            let hue = idx / 10;
            let rem = idx % 10;
            let (light, sat) = if rem < 5 {
                (rem, 1.0)
            } else {
                (rem - 5, 0.5)
            };
            let value = match light {
                0 => 0.35,
                1 => 0.50,
                2 => 0.65,
                3 => 0.80,
                _ => 1.0,
            };
            hsv_to_rgb(hue as f64 * 15.0, sat, value)
        }
    }
}

fn hsv_to_rgb(h_deg: f64, s: f64, v: f64) -> (u8, u8, u8) {
    let h = h_deg.rem_euclid(360.0) / 60.0;
    let c = v * s;
    let x = c * (1.0 - (h.rem_euclid(2.0) - 1.0).abs());
    let m = v - c;
    let (r1, g1, b1) = match h.floor() as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (
        ((r1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((g1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((b1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
    )
}

/// Minimum ISO lineweight (mm) recommended for a colour's contrast vs white.
/// Light colours that still clear 3:1 get a thicker pen floor.
pub fn min_lineweight_mm_for_contrast(contrast: f64) -> f32 {
    if contrast < 3.5 {
        0.50
    } else if contrast < 4.5 {
        0.35
    } else if contrast < 7.0 {
        0.25
    } else {
        0.18
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn black_on_white_is_high_contrast() {
        assert!(contrast_vs_white(0, 0, 0) > 20.0);
    }

    #[test]
    fn aci_7_prints_as_black() {
        assert_eq!(aci_to_srgb(7), (0, 0, 0));
    }
}
