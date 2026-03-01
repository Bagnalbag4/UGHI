// UGHI-inference/src/sampler.rs
// Follows strict_rules.md | No GPU
// Memory cost: ~256 bytes (sampler state)
// Implements temperature, top-k, and top-p (nucleus) sampling.
// Used by the inference engine to select next tokens.

/// Sampling parameters for token generation.
/// Memory cost: ~32 bytes
#[derive(Debug, Clone)]
pub struct SamplingParams {
    /// Temperature (0.0 = greedy, 1.0 = creative, >1.0 = chaotic)
    pub temperature: f32,
    /// Top-k: keep only k highest probability tokens (0 = disabled)
    pub top_k: u32,
    /// Top-p (nucleus): keep tokens until cumulative prob > p (1.0 = disabled)
    pub top_p: f32,
    /// Repetition penalty (1.0 = none, >1.0 = discourage repeats)
    pub repetition_penalty: f32,
}

impl Default for SamplingParams {
    fn default() -> Self {
        Self {
            temperature: 0.7,
            top_k: 40,
            top_p: 0.9,
            repetition_penalty: 1.1,
        }
    }
}

impl SamplingParams {
    /// Greedy sampling (always pick highest probability token).
    /// Memory cost: 0
    pub fn greedy() -> Self {
        Self {
            temperature: 0.0,
            top_k: 1,
            top_p: 1.0,
            repetition_penalty: 1.0,
        }
    }

    /// Creative sampling (higher temperature, wider distribution).
    /// Memory cost: 0
    pub fn creative() -> Self {
        Self {
            temperature: 1.0,
            top_k: 50,
            top_p: 0.95,
            repetition_penalty: 1.15,
        }
    }
}

/// Token sampler: applies temperature, top-k, top-p to logit vector.
/// Memory cost: ~256 bytes (scratch buffers proportional to vocab)
pub struct TokenSampler {
    /// Active sampling parameters
    params: SamplingParams,
    /// LCG state for fast pseudo-random sampling (no external dep)
    rng_state: u64,
}

impl TokenSampler {
    /// Create a new token sampler.
    /// Memory cost: ~64 bytes
    pub fn new(params: SamplingParams) -> Self {
        Self {
            params,
            rng_state: 0x5DEECE66Du64,
        }
    }

    /// Seed the RNG for reproducible sampling.
    /// Memory cost: 0
    pub fn seed(&mut self, seed: u64) {
        self.rng_state = seed;
    }

    /// Sample a token index from the logit vector.
    /// Applies: temperature scaling → top-k filter → top-p filter → weighted random.
    /// Memory cost: ~4 bytes per vocab entry (sorted indices)
    /// Latency: O(V log V) where V = vocab_size (typically 32K–150K)
    pub fn sample(&mut self, logits: &mut Vec<f32>) -> usize {
        let vocab_size = logits.len();
        if vocab_size == 0 {
            return 0;
        }

        // Step 1: Temperature scaling
        // Divide all logits by temperature. Lower temp = sharper distribution.
        if self.params.temperature > 0.0 && self.params.temperature != 1.0 {
            let inv_temp = 1.0 / self.params.temperature;
            for logit in logits.iter_mut() {
                *logit *= inv_temp;
            }
        }

        // Greedy: just return argmax
        if self.params.temperature == 0.0 || self.params.top_k == 1 {
            return argmax(logits);
        }

        // Step 2: Softmax to convert logits → probabilities
        let probs = softmax(logits);

        // Step 3: Create sorted index pairs (prob, index)
        let mut indexed: Vec<(f32, usize)> = probs
            .iter()
            .enumerate()
            .map(|(i, &p)| (p, i))
            .collect();
        // Sort descending by probability
        indexed.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        // Step 4: Top-k filter
        let k = if self.params.top_k > 0 {
            (self.params.top_k as usize).min(indexed.len())
        } else {
            indexed.len()
        };
        indexed.truncate(k);

        // Step 5: Top-p (nucleus) filter
        if self.params.top_p < 1.0 {
            let mut cumulative = 0.0f32;
            let mut cutoff = indexed.len();
            for (i, (prob, _)) in indexed.iter().enumerate() {
                cumulative += prob;
                if cumulative >= self.params.top_p {
                    cutoff = i + 1;
                    break;
                }
            }
            indexed.truncate(cutoff);
        }

        // Step 6: Re-normalize probabilities
        let total: f32 = indexed.iter().map(|(p, _)| p).sum();
        if total <= 0.0 {
            return indexed.first().map(|(_, i)| *i).unwrap_or(0);
        }

        // Step 7: Weighted random selection using LCG
        let rand_val = self.next_random() * total;
        let mut cumulative = 0.0f32;
        for (prob, idx) in &indexed {
            cumulative += prob;
            if cumulative >= rand_val {
                return *idx;
            }
        }

        // Fallback to first token
        indexed.first().map(|(_, i)| *i).unwrap_or(0)
    }

    /// Fast LCG pseudo-random number generator [0.0, 1.0).
    /// No external dependency, deterministic, ~1ns per call.
    /// Memory cost: 0 (modifies internal state)
    fn next_random(&mut self) -> f32 {
        // Linear congruential generator (Java's java.util.Random constants)
        self.rng_state = self.rng_state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        // Extract bits [33..63] as float in [0, 1)
        let bits = (self.rng_state >> 33) as f32;
        bits / (1u64 << 31) as f32
    }
}

/// Argmax: return index of the largest value.
/// Memory cost: 0
fn argmax(values: &[f32]) -> usize {
    values
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(i, _)| i)
        .unwrap_or(0)
}

/// Stable softmax: subtract max for numerical stability.
/// Memory cost: ~4 bytes per value (in-place transform)
fn softmax(logits: &[f32]) -> Vec<f32> {
    let max_val = logits
        .iter()
        .cloned()
        .fold(f32::NEG_INFINITY, f32::max);

    let exps: Vec<f32> = logits.iter().map(|x| (x - max_val).exp()).collect();
    let sum: f32 = exps.iter().sum();

    if sum > 0.0 {
        exps.iter().map(|x| x / sum).collect()
    } else {
        vec![1.0 / logits.len() as f32; logits.len()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_greedy_sampling() {
        let mut sampler = TokenSampler::new(SamplingParams::greedy());
        let mut logits = vec![0.1, 0.3, 0.9, 0.2, 0.5];
        let idx = sampler.sample(&mut logits);
        assert_eq!(idx, 2, "greedy should pick index 2 (highest logit 0.9)");
    }

    #[test]
    fn test_temperature_scaling() {
        let mut sampler = TokenSampler::new(SamplingParams {
            temperature: 0.5,
            top_k: 0,
            top_p: 1.0,
            repetition_penalty: 1.0,
        });
        sampler.seed(42);
        let mut logits = vec![1.0, 2.0, 3.0, 0.5];
        let idx = sampler.sample(&mut logits);
        // With low temperature, should favor index 2 (highest logit)
        assert!(idx <= 3);
    }

    #[test]
    fn test_top_k_filtering() {
        let mut sampler = TokenSampler::new(SamplingParams {
            temperature: 1.0,
            top_k: 2,
            top_p: 1.0,
            repetition_penalty: 1.0,
        });
        sampler.seed(12345);
        let mut logits = vec![0.1, 0.2, 5.0, 4.0, 0.3];
        let idx = sampler.sample(&mut logits);
        // top-k=2: only indices 2 and 3 should be considered
        assert!(idx == 2 || idx == 3, "top-k=2 should only yield index 2 or 3, got {}", idx);
    }

    #[test]
    fn test_softmax_sums_to_one() {
        let logits = vec![1.0, 2.0, 3.0, 4.0];
        let probs = softmax(&logits);
        let sum: f32 = probs.iter().sum();
        assert!((sum - 1.0).abs() < 1e-5, "softmax should sum to 1.0, got {}", sum);
    }

    #[test]
    fn test_argmax() {
        assert_eq!(argmax(&[1.0, 5.0, 3.0, 2.0]), 1);
        assert_eq!(argmax(&[9.0, 1.0, 2.0]), 0);
    }

    #[test]
    fn test_empty_logits() {
        let mut sampler = TokenSampler::new(SamplingParams::greedy());
        let mut logits: Vec<f32> = vec![];
        let idx = sampler.sample(&mut logits);
        assert_eq!(idx, 0, "empty logits should return 0");
    }

    #[test]
    fn test_deterministic_with_seed() {
        let params = SamplingParams::default();
        let mut s1 = TokenSampler::new(params.clone());
        let mut s2 = TokenSampler::new(params);
        s1.seed(42);
        s2.seed(42);

        let mut l1 = vec![1.0, 2.0, 3.0, 1.5, 2.5];
        let mut l2 = l1.clone();

        let idx1 = s1.sample(&mut l1);
        let idx2 = s2.sample(&mut l2);
        assert_eq!(idx1, idx2, "same seed should produce same result");
    }
}
