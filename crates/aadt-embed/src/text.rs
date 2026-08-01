//! Deterministic text embeddings via character n-gram feature hashing.
//!
//! No training, no external dependencies. Architectural vocabulary (layer names,
//! annotation text, block names) embeds such that related terms land nearby:
//! `"WALL"`, `"A-WALL"`, `"WALL-TYPE"` share 3/4-grams and so cluster together.
//!
//! The approach is character n-gram hashing (similar to fastText's subword
//! features, minus the learned table). We extract 2-, 3-, and 4-grams from the
//! lowercased input, hash each with FNV-1a, bucket into `TEXT_DIM` bins, and
//! L2-normalise the result. Identical input → identical vector; deterministic
//! at all call sites.

use aadt_model::{Entity, Geometry};

/// Dimensionality of text embeddings. Separate from [`crate::EMBED_DIM`] (which
/// is for geometry); text needs a wider bucket set for adequate vocabulary coverage.
pub const TEXT_DIM: usize = 64;

// ── FNV-1a constants ──────────────────────────────────────────────────────────

const FNV_OFFSET: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x00000100000001b3;

fn fnv1a(s: &str) -> u64 {
    let mut h = FNV_OFFSET;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(FNV_PRIME);
    }
    h
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Embed `s` into a `TEXT_DIM`-dimensional L2-normalised vector using character
/// n-gram hashing (2-, 3-, 4-grams). Returns the zero vector when `s` is blank.
pub fn text_embedding(s: &str) -> [f32; TEXT_DIM] {
    let lower = s.to_ascii_lowercase();
    let chars: Vec<char> = lower.chars().collect();
    let n = chars.len();
    let mut acc = [0.0f32; TEXT_DIM];

    for start in 0..n {
        for len in 2..=4usize {
            if start + len > n {
                break;
            }
            let gram: String = chars[start..start + len].iter().collect();
            let h = fnv1a(&gram) as usize;
            // Use two independent buckets per gram to reduce collision rate.
            acc[h % TEXT_DIM] += 1.0;
            let h2 = fnv1a(&format!("{gram}#")) as usize;
            acc[h2 % TEXT_DIM] += 0.5;
        }
    }

    l2_normalize(&mut acc);
    acc
}

/// The human-readable text label of an entity: its layer name, plus the
/// annotation text if the entity carries [`Geometry::Text`] geometry.
pub fn entity_text_label(entity: &Entity) -> String {
    let mut s = entity.layer.clone();
    if let Geometry::Text(t) = &entity.geometry {
        if !t.value.is_empty() {
            s.push(' ');
            s.push_str(&t.value);
        }
    }
    s
}

fn l2_normalize(v: &mut [f32; TEXT_DIM]) {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 1e-12 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_input_is_deterministic() {
        let a = text_embedding("A-WALL");
        let b = text_embedding("A-WALL");
        assert_eq!(a, b);
    }

    #[test]
    fn output_is_unit_length() {
        for s in ["ROOM", "A-WALL", "BEDROOM", "DOOR-TYPE-A", ""] {
            let v = text_embedding(s);
            let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
            if s.is_empty() {
                assert_eq!(norm, 0.0, "empty string → zero vector");
            } else {
                assert!((norm - 1.0).abs() < 1e-4, "not unit: norm={norm} for {s:?}");
            }
        }
    }

    #[test]
    fn related_terms_are_close() {
        let wall = text_embedding("WALL");
        let a_wall = text_embedding("A-WALL");
        let wall_type = text_embedding("WALL-TYPE");
        let room = text_embedding("ROOM");

        let cos = |a: &[f32; TEXT_DIM], b: &[f32; TEXT_DIM]| -> f32 {
            a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
        };
        let wall_awall = cos(&wall, &a_wall);
        let wall_walltype = cos(&wall, &wall_type);
        let wall_room = cos(&wall, &room);

        assert!(
            wall_awall > wall_room,
            "A-WALL should be closer to WALL than ROOM is ({wall_awall:.3} vs {wall_room:.3})"
        );
        assert!(
            wall_walltype > wall_room,
            "WALL-TYPE should be closer to WALL than ROOM is ({wall_walltype:.3} vs {wall_room:.3})"
        );
    }

    #[test]
    fn distinct_terms_have_low_similarity() {
        let wall = text_embedding("WALL");
        let stair = text_embedding("STAIR");
        let cos: f32 = wall.iter().zip(stair.iter()).map(|(x, y)| x * y).sum();
        assert!(
            cos < 0.7,
            "WALL and STAIR should not be too similar: {cos:.3}"
        );
    }
}
