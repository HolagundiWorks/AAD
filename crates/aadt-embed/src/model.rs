//! A small multilayer perceptron — the "local ML model" — trained deterministically
//! at first use on seeded synthetic exemplars. One hidden `tanh` layer doubles as
//! the embedding; a linear + softmax head produces class probabilities.
//!
//! Everything is plain `f32` arithmetic (no `unsafe`, no external crate). Training
//! is reproducible: fixed seed, fixed synthetic dataset, fixed iteration order.

use std::sync::OnceLock;

use aadt_model::Document;

use crate::classes::ElementClass;
use crate::features::{features_for_geometry, synth_geometry, FEATURE_DIM};
use crate::rng::Rng;

/// Hidden-layer width, which is also the embedding dimensionality.
pub const HIDDEN: usize = 32;
/// Number of output classes (see [`ElementClass::ALL`]).
pub const NUM_CLASSES: usize = 10;
/// Embedding vector length (equals [`HIDDEN`]).
pub const EMBED_DIM: usize = HIDDEN;

/// Identifier for the trained model — bumped when the architecture or training
/// changes, so persisted embeddings from an older model are recomputed.
pub const MODEL_ID: &str = "aadt-embed-mlp-1/f25-h32-c10";

const SEED: u64 = 0x5AAD_7E11_C0DE_0001; // fixed training seed
const SAMPLES_PER_CLASS: usize = 120;
const EPOCHS: usize = 60;
const LR: f32 = 0.2;

/// The trained network weights.
#[derive(Debug, Clone)]
pub struct Model {
    w1: Vec<f32>, // HIDDEN x FEATURE_DIM
    b1: Vec<f32>, // HIDDEN
    w2: Vec<f32>, // NUM_CLASSES x HIDDEN
    b2: Vec<f32>, // NUM_CLASSES
}

/// Result of running the model on one element.
pub struct Inference {
    /// Argmax class index into [`ElementClass::ALL`].
    pub class: usize,
    /// Softmax probability of the winning class (the confidence).
    pub confidence: f32,
    /// Full softmax distribution over classes (index = class index).
    pub probs: [f32; NUM_CLASSES],
    /// L2-normalized hidden activations — the stored embedding.
    pub embedding: Vec<f32>,
}

impl Model {
    /// Forward pass. Returns `(hidden tanh activations, softmax probabilities)`.
    fn forward(&self, x: &[f32]) -> ([f32; HIDDEN], [f32; NUM_CLASSES]) {
        let mut h = [0.0f32; HIDDEN];
        for (j, hj) in h.iter_mut().enumerate() {
            let mut z = self.b1[j];
            let row = j * FEATURE_DIM;
            for (i, &xi) in x.iter().enumerate().take(FEATURE_DIM) {
                z += self.w1[row + i] * xi;
            }
            *hj = z.tanh();
        }
        let mut logits = [0.0f32; NUM_CLASSES];
        for (k, lk) in logits.iter_mut().enumerate() {
            let mut z = self.b2[k];
            let row = k * HIDDEN;
            for (j, &hj) in h.iter().enumerate() {
                z += self.w2[row + j] * hj;
            }
            *lk = z;
        }
        (h, softmax(&logits))
    }

    /// Classify a feature vector into a class + confidence + embedding.
    pub fn infer(&self, x: &[f32]) -> Inference {
        let (h, p) = self.forward(x);
        let mut best = 0usize;
        for k in 1..NUM_CLASSES {
            if p[k] > p[best] {
                best = k;
            }
        }
        Inference {
            class: best,
            confidence: p[best],
            probs: p,
            embedding: l2_normalize(&h),
        }
    }

    /// Just the L2-normalized embedding for a feature vector.
    pub fn embed(&self, x: &[f32]) -> Vec<f32> {
        let (h, _) = self.forward(x);
        l2_normalize(&h)
    }

    /// Train the default model deterministically from seeded synthetic exemplars.
    pub fn train_default() -> Model {
        let mut rng = Rng::new(SEED);
        let doc = Document::new();

        // Build the labeled synthetic dataset.
        let mut xs: Vec<[f32; FEATURE_DIM]> = Vec::new();
        let mut ys: Vec<usize> = Vec::new();
        for class in ElementClass::ALL {
            for _ in 0..SAMPLES_PER_CLASS {
                let g = synth_geometry(class, &mut rng);
                let f = features_for_geometry(&g, &doc);
                let mut arr = [0.0f32; FEATURE_DIM];
                arr.copy_from_slice(&f);
                xs.push(arr);
                ys.push(class.index());
            }
        }

        // Seeded weight init (scaled uniform ~ Xavier).
        let s1 = 1.0 / (FEATURE_DIM as f32).sqrt();
        let s2 = 1.0 / (HIDDEN as f32).sqrt();
        let mut model = Model {
            w1: (0..HIDDEN * FEATURE_DIM)
                .map(|_| rng.normal() * s1)
                .collect(),
            b1: vec![0.0; HIDDEN],
            w2: (0..NUM_CLASSES * HIDDEN)
                .map(|_| rng.normal() * s2)
                .collect(),
            b2: vec![0.0; NUM_CLASSES],
        };

        let n = xs.len();
        let mut order: Vec<usize> = (0..n).collect();
        for _ in 0..EPOCHS {
            shuffle(&mut order, &mut rng);
            for &idx in &order {
                model.sgd_step(&xs[idx], ys[idx]);
            }
        }
        model
    }

    /// One stochastic-gradient-descent update on a single (x, label) pair.
    fn sgd_step(&mut self, x: &[f32], y: usize) {
        let (h, p) = self.forward(x);

        // dL/dlogits = p - onehot(y).
        let mut dlogits = p;
        dlogits[y] -= 1.0;

        // Gradient into the hidden layer.
        let mut dh = [0.0f32; HIDDEN];
        for (k, &dl) in dlogits.iter().enumerate() {
            let row = k * HIDDEN;
            for (j, dhj) in dh.iter_mut().enumerate() {
                *dhj += self.w2[row + j] * dl;
            }
        }

        // Update W2 / b2.
        for (k, &dl) in dlogits.iter().enumerate() {
            let row = k * HIDDEN;
            for (j, &hj) in h.iter().enumerate() {
                self.w2[row + j] -= LR * dl * hj;
            }
            self.b2[k] -= LR * dl;
        }

        // Backprop through tanh and update W1 / b1.
        for (j, &hj) in h.iter().enumerate() {
            let dz = dh[j] * (1.0 - hj * hj);
            let row = j * FEATURE_DIM;
            for (i, &xi) in x.iter().enumerate() {
                self.w1[row + i] -= LR * dz * xi;
            }
            self.b1[j] -= LR * dz;
        }
    }
}

fn softmax(logits: &[f32; NUM_CLASSES]) -> [f32; NUM_CLASSES] {
    let max = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let mut out = [0.0f32; NUM_CLASSES];
    let mut sum = 0.0f32;
    for (o, &l) in out.iter_mut().zip(logits.iter()) {
        *o = (l - max).exp();
        sum += *o;
    }
    if sum > 0.0 {
        for o in out.iter_mut() {
            *o /= sum;
        }
    }
    out
}

fn l2_normalize(v: &[f32]) -> Vec<f32> {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm <= 1e-12 {
        return v.to_vec();
    }
    v.iter().map(|x| x / norm).collect()
}

/// Deterministic Fisher–Yates shuffle driven by the seeded PRNG.
fn shuffle(order: &mut [usize], rng: &mut Rng) {
    let n = order.len();
    for i in (1..n).rev() {
        let j = rng.below(i + 1);
        order.swap(i, j);
    }
}

/// The process-wide default model, trained once on first access and cached.
pub fn default_model() -> &'static Model {
    static MODEL: OnceLock<Model> = OnceLock::new();
    MODEL.get_or_init(Model::train_default)
}

/// Centroid embedding of a class, computed from seeded synthetic exemplars.
#[derive(Debug, Clone)]
pub struct ClassPrototype {
    /// Class index into [`ElementClass::ALL`].
    pub class: usize,
    /// L2-normalized mean embedding for the class.
    pub vector: Vec<f32>,
}

const PROTO_SAMPLES: usize = 100;

/// Per-class prototype embeddings — the labeled reference vectors the vector DB
/// seeds itself with. Computed once (using the default model) and cached.
pub fn default_prototypes() -> &'static Vec<ClassPrototype> {
    static PROTOS: OnceLock<Vec<ClassPrototype>> = OnceLock::new();
    PROTOS.get_or_init(|| {
        let m = default_model();
        let mut rng = Rng::new(SEED ^ 0x0000_ABCD_0000_1234);
        let doc = Document::new();
        let mut out = Vec::with_capacity(NUM_CLASSES);
        for class in ElementClass::ALL {
            let mut acc = vec![0.0f32; EMBED_DIM];
            for _ in 0..PROTO_SAMPLES {
                let g = synth_geometry(class, &mut rng);
                let f = features_for_geometry(&g, &doc);
                for (a, v) in acc.iter_mut().zip(m.embed(&f)) {
                    *a += v;
                }
            }
            let norm = acc.iter().map(|x| x * x).sum::<f32>().sqrt();
            if norm > 1e-12 {
                for a in acc.iter_mut() {
                    *a /= norm;
                }
            }
            out.push(ClassPrototype {
                class: class.index(),
                vector: acc,
            });
        }
        out
    })
}
