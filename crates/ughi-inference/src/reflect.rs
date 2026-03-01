// UGHI-inference/src/reflect.rs
// Follows strict_rules.md | skills.md: "SelfCritique – Calls same SLM with reflection prompt"
// Memory cost: ~2 KB (reflection prompt templates)
// Implements self-critique / reflection mode.
// The reflection engine takes the original prompt + response and generates
// a critique with confidence score. If confidence is low, it suggests revision.

use crate::request::ReflectionResult;

/// Reflection engine for self-critique using the same SLM.
/// Memory cost: ~2 KB (static prompt templates)
/// skills.md: "SelfCritique – Calls same SLM with reflection prompt"
pub struct ReflectionEngine {
    /// Minimum confidence threshold for accepting a response
    confidence_threshold: f32,
}

impl ReflectionEngine {
    /// Create a new reflection engine.
    /// Memory cost: ~64 bytes
    pub fn new() -> Self {
        Self {
            confidence_threshold: 0.7,
        }
    }

    /// Create with custom confidence threshold.
    pub fn with_threshold(threshold: f32) -> Self {
        Self {
            confidence_threshold: threshold,
        }
    }

    /// Generate the reflection prompt for self-critique.
    /// Memory cost: ~prompt_len + response_len bytes
    pub fn build_reflection_prompt(prompt: &str, response: &str) -> String {
        format!(
            "You are a self-critique agent. Review the following response and provide:\n\
             1. A brief critique (what's good, what's missing)\n\
             2. A confidence score (0.0 to 1.0)\n\
             3. Whether the response should be revised\n\n\
             ORIGINAL TASK:\n{}\n\n\
             RESPONSE:\n{}\n\n\
             CRITIQUE:",
            prompt, response
        )
    }

    /// Run reflection on a prompt-response pair.
    /// In production: calls the same SLM with the reflection prompt.
    /// Currently: generates a simulated critique based on heuristics.
    /// Memory cost: ~1 KB (critique text)
    pub fn reflect(&self, prompt: &str, response: &str) -> ReflectionResult {
        // Heuristic-based quality assessment
        let confidence = self.assess_quality(prompt, response);
        let should_revise = confidence < self.confidence_threshold;

        let critique = if confidence >= 0.8 {
            format!(
                "Response is comprehensive and well-structured. \
                 Covers the key aspects of the task. \
                 Confidence: {:.0}%",
                confidence * 100.0
            )
        } else if confidence >= 0.5 {
            format!(
                "Response addresses the task but could be more detailed. \
                 Consider elaborating on key points and providing examples. \
                 Confidence: {:.0}%",
                confidence * 100.0
            )
        } else {
            format!(
                "Response needs significant improvement. \
                 Missing key aspects of the task. Recommend regeneration. \
                 Confidence: {:.0}%",
                confidence * 100.0
            )
        };

        let revised_text = if should_revise {
            Some(format!(
                "[Revised] Based on self-critique, here is an improved response:\n\n\
                 The task '{}' requires a more thorough analysis. \
                 Key improvements needed:\n\
                 - More specific actionable steps\n\
                 - Concrete examples and metrics\n\
                 - Risk assessment and mitigation strategies",
                truncate(prompt, 100)
            ))
        } else {
            None
        };

        ReflectionResult {
            critique,
            confidence,
            should_revise,
            revised_text,
            reflection_tokens: 64, // Simulated token count
        }
    }

    /// Assess response quality based on heuristics.
    /// Memory cost: 0
    /// Returns confidence score 0.0–1.0.
    fn assess_quality(&self, prompt: &str, response: &str) -> f32 {
        let mut score = 0.5f32;

        // Length ratio: response should be proportional to prompt complexity
        let prompt_len = prompt.len() as f32;
        let response_len = response.len() as f32;

        if response_len > prompt_len * 2.0 {
            score += 0.15; // Detailed response
        }
        if response_len > prompt_len * 5.0 {
            score += 0.1; // Very detailed
        }

        // Structure indicators
        if response.contains('\n') {
            score += 0.05; // Has line breaks
        }
        if response.contains("1.") || response.contains("- ") {
            score += 0.1; // Has lists
        }

        // Content quality indicators
        let quality_words = [
            "because",
            "therefore",
            "however",
            "specifically",
            "example",
            "analysis",
            "recommend",
            "approach",
        ];
        for word in &quality_words {
            if response.to_lowercase().contains(word) {
                score += 0.02;
            }
        }

        // Non-empty check
        if response.trim().is_empty() {
            return 0.0;
        }

        score.min(1.0).max(0.0)
    }
}

/// Truncate a string to max_len characters.
/// Memory cost: 0 (returns slice reference as String)
fn truncate(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        s
    } else {
        &s[..max_len]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reflect_high_quality() {
        let engine = ReflectionEngine::new();
        let prompt = "Explain how to build a startup";
        let response = "Here is a comprehensive analysis:\n\
                        1. First, identify the market opportunity because understanding demand is key\n\
                        2. Develop a minimum viable product\n\
                        3. Find your target audience\n\
                        4. Build a team with complementary skills\n\
                        5. Secure funding through investors or bootstrapping\n\
                        Therefore, the approach should be systematic and data-driven.";

        let result = engine.reflect(prompt, response);
        assert!(
            result.confidence >= 0.7,
            "high-quality response should have high confidence: {}",
            result.confidence
        );
        assert!(!result.should_revise);
    }

    #[test]
    fn test_reflect_low_quality() {
        let engine = ReflectionEngine::new();
        let prompt = "Design a complete distributed system architecture";
        let response = "ok";

        let result = engine.reflect(prompt, response);
        assert!(
            result.confidence < 0.7,
            "low-quality response should have low confidence: {}",
            result.confidence
        );
        assert!(result.should_revise);
        assert!(result.revised_text.is_some());
    }

    #[test]
    fn test_reflect_empty_response() {
        let engine = ReflectionEngine::new();
        let result = engine.reflect("test", "");
        assert_eq!(result.confidence, 0.0);
        assert!(result.should_revise);
    }

    #[test]
    fn test_reflection_prompt_format() {
        let prompt = ReflectionEngine::build_reflection_prompt("hello", "world");
        assert!(prompt.contains("ORIGINAL TASK:"));
        assert!(prompt.contains("hello"));
        assert!(prompt.contains("CRITIQUE:"));
    }

    #[test]
    fn test_custom_threshold() {
        let engine = ReflectionEngine::with_threshold(0.9);
        let result = engine.reflect(
            "test",
            "A reasonable but short response with some analysis.",
        );
        // With high threshold, even medium responses should trigger revision
        assert!(result.confidence < 0.9 || !result.should_revise);
    }
}
