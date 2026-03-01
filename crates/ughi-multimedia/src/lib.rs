// ughi-multimedia/src/lib.rs
// Follows strict_rules.md | skills.md
// Hybrid Image + Video Generation Engine
// Memory: ~45 MB peak per generation | Zero GPU local default
// Built-in skills: ImageGen, VideoGen (skills.md #11, #12)
//
// Usage:
//   ughi generate image "cyberpunk UGHI logo neon style"
//   ughi generate video "robot walking in 2026 Tokyo rain"
//
// Modes:
//   --model=local   Force local (no API key needed)
//   --model=cloud   Force cloud (requires API key)
//   --model=auto    Auto-detect (default)

pub mod config;
pub mod image;
pub mod video;

pub use config::{CloudProvider, EncryptedKey, ModelMode, MultimediaConfig};
pub use image::{
    AspectRatio, ImageGenMetrics, ImageGenRequest, ImageGenResult, ImageGenerator, ImageStyle,
};
pub use video::{
    MotionStyle, VideoGenMetrics, VideoGenRequest, VideoGenResult, VideoGenerator, VideoQuality,
};

/// Unified multimedia engine (image + video + config).
/// Memory cost: ~1 KB base, ~45 MB peak during generation.
pub struct MultimediaEngine {
    pub config: MultimediaConfig,
    pub image_gen: ImageGenerator,
    pub video_gen: VideoGenerator,
}

impl MultimediaEngine {
    /// Create with default config (local mode, no API keys).
    pub fn new() -> Self {
        Self {
            config: MultimediaConfig::new(),
            image_gen: ImageGenerator::new(),
            video_gen: VideoGenerator::new(),
        }
    }

    /// Generate an image.
    /// CLI: `ughi generate image "prompt"`
    pub fn generate_image(
        &mut self,
        request: &ImageGenRequest,
    ) -> Result<ImageGenResult, String> {
        self.image_gen.generate(request, &self.config)
    }

    /// Generate a video.
    /// CLI: `ughi generate video "prompt"`
    pub fn generate_video(
        &mut self,
        request: &VideoGenRequest,
    ) -> Result<VideoGenResult, String> {
        self.video_gen.generate(request, &self.config)
    }

    /// Set an API key.
    /// CLI: `ughi config set api.grok.key <key>`
    pub fn set_api_key(&mut self, provider: CloudProvider, key: &str) {
        self.config.set_key(provider, key);
    }

    /// Get the effective mode for a request type.
    pub fn effective_mode(&self, is_image: bool) -> ModelMode {
        self.config.resolve_mode(is_image)
    }

    /// Combined metrics.
    pub fn metrics(&self) -> MultimediaMetrics {
        MultimediaMetrics {
            config: self.config.metrics(),
            image: self.image_gen.metrics(),
            video: self.video_gen.metrics(),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MultimediaMetrics {
    pub config: config::ConfigMetrics,
    pub image: ImageGenMetrics,
    pub video: VideoGenMetrics,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engine_local_image() {
        let mut engine = MultimediaEngine::new();
        let req = ImageGenRequest::new("UGHI logo cyberpunk")
            .with_style(ImageStyle::Cyberpunk);
        let result = engine.generate_image(&req).unwrap();
        assert_eq!(result.mode, "local");
    }

    #[test]
    fn test_engine_local_video() {
        let mut engine = MultimediaEngine::new();
        let req = VideoGenRequest::new("robot walking in rain")
            .with_duration(5);
        let result = engine.generate_video(&req).unwrap();
        assert_eq!(result.mode, "local");
        assert_eq!(result.duration_secs, 5);
    }

    #[test]
    fn test_engine_auto_upgrade_with_key() {
        let mut engine = MultimediaEngine::new();
        assert_eq!(engine.effective_mode(true), ModelMode::Local);

        engine.set_api_key(CloudProvider::GrokImagine, "xai-key");
        assert_eq!(engine.effective_mode(true), ModelMode::Cloud);
        // Video still local (no video key)
        assert_eq!(engine.effective_mode(false), ModelMode::Local);
    }

    #[test]
    fn test_engine_metrics() {
        let mut engine = MultimediaEngine::new();
        engine.generate_image(&ImageGenRequest::new("test")).unwrap();
        engine.generate_video(&VideoGenRequest::new("test")).unwrap();

        let m = engine.metrics();
        assert_eq!(m.image.total_generated, 1);
        assert_eq!(m.video.total_generated, 1);
    }

    #[test]
    fn test_api_key_never_leaked() {
        let mut engine = MultimediaEngine::new();
        engine.set_api_key(CloudProvider::GrokImagine, "super-secret-key-xyz");

        // Key must decrypt correctly
        assert_eq!(
            engine.config.get_key(CloudProvider::GrokImagine).unwrap(),
            "super-secret-key-xyz"
        );

        // Debug output must NOT contain the key
        let debug = format!("{:?}", engine.metrics());
        assert!(!debug.contains("super-secret-key-xyz"));
    }
}
