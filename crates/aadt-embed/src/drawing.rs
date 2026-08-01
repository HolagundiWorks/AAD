//! Drawing-level and object-level aggregate embeddings.
//!
//! Both functions mean-pool a collection of L2-normalised element embeddings and
//! L2-normalise the result, producing a single vector that captures the overall
//! geometric character of a drawing or an intelligent object's member elements.
//!
//! Mean-pooling is translation-invariant (already handled by element features)
//! and preserves the similarity structure of the per-element space, so cosine
//! search in the aggregate space remains meaningful.

/// Produce a drawing-level embedding by mean-pooling `vecs` and L2-normalising.
/// Returns a zero vector (of the same length as the first slice) when `vecs` is
/// empty or every slice is empty.
pub fn drawing_embedding(vecs: &[&[f32]]) -> Vec<f32> {
    aggregate(vecs)
}

/// Produce an object-level embedding by mean-pooling the member element vectors.
/// Semantically equivalent to [`drawing_embedding`]; the separate name clarifies
/// intent at call sites.
pub fn object_embedding(vecs: &[&[f32]]) -> Vec<f32> {
    aggregate(vecs)
}

fn aggregate(vecs: &[&[f32]]) -> Vec<f32> {
    let Some(&first) = vecs.first() else {
        return Vec::new();
    };
    let dim = first.len();
    if dim == 0 {
        return Vec::new();
    }
    let mut acc = vec![0.0f32; dim];
    let mut count = 0usize;
    for v in vecs {
        if v.len() == dim {
            for (a, &x) in acc.iter_mut().zip(v.iter()) {
                *a += x;
            }
            count += 1;
        }
    }
    if count == 0 {
        return vec![0.0; dim];
    }
    for a in acc.iter_mut() {
        *a /= count as f32;
    }
    let norm = acc.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 1e-12 {
        for a in acc.iter_mut() {
            *a /= norm;
        }
    }
    acc
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(v: &[f32]) -> Vec<f32> {
        let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        v.iter().map(|x| x / n).collect()
    }

    #[test]
    fn empty_returns_empty() {
        assert!(drawing_embedding(&[]).is_empty());
    }

    #[test]
    fn single_vector_returns_normalized_copy() {
        let v = unit(&[3.0, 4.0]);
        let out = drawing_embedding(&[v.as_slice()]);
        let norm: f32 = out.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5);
    }

    #[test]
    fn identical_vectors_return_same_direction() {
        let v = unit(&[1.0, 1.0]);
        let out = drawing_embedding(&[v.as_slice(), v.as_slice(), v.as_slice()]);
        let cos: f32 = out.iter().zip(v.iter()).map(|(a, b)| a * b).sum();
        assert!((cos - 1.0).abs() < 1e-5);
    }

    #[test]
    fn mismatched_dim_slices_skipped() {
        let v2 = unit(&[1.0, 0.0]);
        let v3 = vec![1.0, 0.0, 0.0]; // different dim
                                      // Only v2 counts; result should be unit on the 2-dim space.
        let out = drawing_embedding(&[v2.as_slice(), v3.as_slice()]);
        assert_eq!(out.len(), 2);
        let norm: f32 = out.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5);
    }
}
