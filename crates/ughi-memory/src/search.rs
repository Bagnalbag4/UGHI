// UGHI-memory/src/search.rs
// Follows strict_rules.md | Pure Rust cosine similarity
// Memory cost: O(n) where n = number of entries searched
// No external vector DB dep – pure math for ≤18 MB binary target.

/// Cosine similarity between two vectors.
/// Returns value in [-1.0, 1.0]. Higher = more similar.
/// Memory cost: 0 (in-place computation)
/// Latency: O(d) where d = dimension (typically 128 or 384)
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;

    for i in 0..a.len() {
        dot += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }

    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom < 1e-8 {
        return 0.0;
    }

    dot / denom
}

/// Normalize a vector to unit length (L2 norm).
/// Memory cost: 0 (in-place mutation)
pub fn normalize(v: &mut [f32]) {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 1e-8 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

/// Generate a simple hash-based embedding for text (for testing/demo).
/// In production, use the SLM's hidden states as embeddings.
/// Memory cost: dim * 4 bytes
pub fn simple_text_embedding(text: &str, dim: usize) -> Vec<f32> {
    let mut emb = vec![0.0f32; dim];
    let bytes = text.as_bytes();

    for (i, &b) in bytes.iter().enumerate() {
        let idx = (b as usize * 7 + i * 13) % dim;
        emb[idx] += (b as f32 - 96.0) / 128.0;
    }

    normalize(&mut emb);
    emb
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_identical() {
        let a = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity(&a, &a);
        assert!((sim - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_cosine_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-5);
    }

    #[test]
    fn test_cosine_opposite() {
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - (-1.0)).abs() < 1e-5);
    }

    #[test]
    fn test_cosine_dimension_mismatch() {
        let a = vec![1.0, 2.0];
        let b = vec![1.0];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn test_normalize() {
        let mut v = vec![3.0, 4.0];
        normalize(&mut v);
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_text_embedding() {
        let emb = simple_text_embedding("hello world", 128);
        assert_eq!(emb.len(), 128);
        let norm: f32 = emb.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-4, "embedding should be normalized");
    }

    #[test]
    fn test_similar_texts() {
        let e1 = simple_text_embedding("startup plan", 128);
        let e2 = simple_text_embedding("startup strategy", 128);
        let e3 = simple_text_embedding("weather forecast", 128);

        let sim_related = cosine_similarity(&e1, &e2);
        let sim_unrelated = cosine_similarity(&e1, &e3);
        assert!(
            sim_related > sim_unrelated,
            "related texts should be more similar"
        );
    }
}
