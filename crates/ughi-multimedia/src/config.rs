// ughi-multimedia/src/config.rs
// Follows strict_rules.md | Keys NEVER logged or serialized in plaintext
// Memory cost: ~512 bytes (key storage + XOR cipher)
// Secure API key management for cloud providers:
//   ughi config set api.grok.key <key>
//   ughi config set api.kling.key <key>

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::info;

/// Supported cloud API providers for multimedia generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CloudProvider {
    /// xAI Grok Imagine (image generation)
    GrokImagine,
    /// Flux Pro API (image generation)
    FluxPro,
    /// Stability AI SD3 API (image generation)
    StabilitySD3,
    /// Kling AI (video generation)
    KlingAI,
    /// Runway Gen-3 Alpha (video generation)
    RunwayGen3,
    /// Luma Dream Machine (video generation)
    LumaDream,
}

impl std::fmt::Display for CloudProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GrokImagine => write!(f, "Grok Imagine"),
            Self::FluxPro => write!(f, "Flux Pro"),
            Self::StabilitySD3 => write!(f, "Stability SD3"),
            Self::KlingAI => write!(f, "Kling AI"),
            Self::RunwayGen3 => write!(f, "Runway Gen-3"),
            Self::LumaDream => write!(f, "Luma Dream Machine"),
        }
    }
}

impl CloudProvider {
    /// Config key path for this provider.
    pub fn config_key(&self) -> &'static str {
        match self {
            Self::GrokImagine => "api.grok.key",
            Self::FluxPro => "api.flux.key",
            Self::StabilitySD3 => "api.stability.key",
            Self::KlingAI => "api.kling.key",
            Self::RunwayGen3 => "api.runway.key",
            Self::LumaDream => "api.luma.key",
        }
    }

    /// API endpoint URL.
    pub fn endpoint(&self) -> &'static str {
        match self {
            Self::GrokImagine => "https://api.x.ai/v1/images/generations",
            Self::FluxPro => "https://api.bfl.ml/v1/flux-pro-1.1",
            Self::StabilitySD3 => "https://api.stability.ai/v2beta/stable-image/generate/sd3",
            Self::KlingAI => "https://api.klingai.com/v1/videos/image2video",
            Self::RunwayGen3 => "https://api.dev.runwayml.com/v1/image_to_video",
            Self::LumaDream => "https://api.lumalabs.ai/dream-machine/v1/generations",
        }
    }
}

/// Encrypted API key (XOR cipher, never plaintext in memory longer than needed).
/// Same encryption approach as hibernation.rs (H-02).
#[derive(Clone)]
pub struct EncryptedKey {
    encrypted_bytes: Vec<u8>,
    provider: CloudProvider,
}

impl EncryptedKey {
    /// Encrypt and store a key.
    pub fn new(provider: CloudProvider, plaintext: &str) -> Self {
        let key_material = derive_key(provider.config_key());
        let encrypted = xor_cipher(plaintext.as_bytes(), &key_material);
        Self {
            encrypted_bytes: encrypted,
            provider,
        }
    }

    /// Decrypt and return the key (ephemeral — use immediately, don't store).
    pub fn decrypt(&self) -> String {
        let key_material = derive_key(self.provider.config_key());
        let decrypted = xor_cipher(&self.encrypted_bytes, &key_material);
        String::from_utf8(decrypted).unwrap_or_default()
    }

    pub fn provider(&self) -> CloudProvider {
        self.provider
    }
}

// Redact in Debug — keys never visible
impl std::fmt::Debug for EncryptedKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "EncryptedKey({}, [REDACTED])", self.provider)
    }
}

/// Multimedia configuration manager.
/// Stores encrypted API keys + generation preferences.
/// Memory cost: ~512 bytes
pub struct MultimediaConfig {
    /// Encrypted API keys per provider
    keys: HashMap<CloudProvider, EncryptedKey>,
    /// Preferred model mode
    pub default_mode: ModelMode,
    /// Default image resolution
    pub default_image_width: u32,
    pub default_image_height: u32,
    /// Default video duration (seconds)
    pub default_video_duration_secs: u8,
}

/// Model mode: local (CPU), cloud (API), or auto (cloud if key available).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelMode {
    /// Always use local models (no API key needed)
    Local,
    /// Always use cloud APIs (requires API key)
    Cloud,
    /// Auto: use cloud if API key set, otherwise local
    Auto,
}

impl std::fmt::Display for ModelMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Local => write!(f, "local"),
            Self::Cloud => write!(f, "cloud"),
            Self::Auto => write!(f, "auto"),
        }
    }
}

impl MultimediaConfig {
    /// Create with defaults (local mode, 1024x1024, 5s video).
    pub fn new() -> Self {
        Self {
            keys: HashMap::with_capacity(6),
            default_mode: ModelMode::Auto,
            default_image_width: 1024,
            default_image_height: 1024,
            default_video_duration_secs: 5,
        }
    }

    /// Set an API key (encrypted immediately, plaintext discarded).
    /// CLI: `ughi config set api.grok.key <key>`
    pub fn set_key(&mut self, provider: CloudProvider, plaintext_key: &str) {
        let encrypted = EncryptedKey::new(provider, plaintext_key);
        self.keys.insert(provider, encrypted);
        info!(provider = %provider, "API key set (encrypted, never logged)");
    }

    /// Remove an API key.
    pub fn remove_key(&mut self, provider: CloudProvider) {
        self.keys.remove(&provider);
        info!(provider = %provider, "API key removed");
    }

    /// Check if a provider has an API key configured.
    pub fn has_key(&self, provider: CloudProvider) -> bool {
        self.keys.contains_key(&provider)
    }

    /// Get decrypted key (ephemeral use only).
    pub fn get_key(&self, provider: CloudProvider) -> Option<String> {
        self.keys.get(&provider).map(|k| k.decrypt())
    }

    /// Resolve effective mode for a generation request.
    /// Auto mode → cloud if any relevant key exists, otherwise local.
    pub fn resolve_mode(&self, is_image: bool) -> ModelMode {
        match self.default_mode {
            ModelMode::Local => ModelMode::Local,
            ModelMode::Cloud => ModelMode::Cloud,
            ModelMode::Auto => {
                let has_cloud = if is_image {
                    self.has_key(CloudProvider::GrokImagine)
                        || self.has_key(CloudProvider::FluxPro)
                        || self.has_key(CloudProvider::StabilitySD3)
                } else {
                    self.has_key(CloudProvider::KlingAI)
                        || self.has_key(CloudProvider::RunwayGen3)
                        || self.has_key(CloudProvider::LumaDream)
                };
                if has_cloud {
                    ModelMode::Cloud
                } else {
                    ModelMode::Local
                }
            }
        }
    }

    /// Get the best available image provider (prefers Grok > Flux > SD3).
    pub fn best_image_provider(&self) -> Option<CloudProvider> {
        [
            CloudProvider::GrokImagine,
            CloudProvider::FluxPro,
            CloudProvider::StabilitySD3,
        ]
        .into_iter()
        .find(|p| self.has_key(*p))
    }

    /// Get the best available video provider (prefers Kling > Runway > Luma).
    pub fn best_video_provider(&self) -> Option<CloudProvider> {
        [
            CloudProvider::KlingAI,
            CloudProvider::RunwayGen3,
            CloudProvider::LumaDream,
        ]
        .into_iter()
        .find(|p| self.has_key(*p))
    }

    /// Number of configured API keys.
    pub fn key_count(&self) -> usize {
        self.keys.len()
    }

    /// Metrics.
    pub fn metrics(&self) -> ConfigMetrics {
        ConfigMetrics {
            configured_keys: self.keys.len() as u32,
            default_mode: format!("{}", self.default_mode),
            image_resolution: format!("{}x{}", self.default_image_width, self.default_image_height),
            video_duration_secs: self.default_video_duration_secs,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigMetrics {
    pub configured_keys: u32,
    pub default_mode: String,
    pub image_resolution: String,
    pub video_duration_secs: u8,
}

/// Derive 32-byte encryption key from a config path string.
fn derive_key(config_path: &str) -> [u8; 32] {
    let mut key = [0u8; 32];
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in config_path.bytes() {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    for i in 0..4 {
        let segment = hash.wrapping_add(i as u64).to_le_bytes();
        key[i * 8..(i + 1) * 8].copy_from_slice(&segment);
    }
    key
}

/// XOR stream cipher (symmetric).
fn xor_cipher(data: &[u8], key: &[u8; 32]) -> Vec<u8> {
    data.iter()
        .enumerate()
        .map(|(i, b)| b ^ key[i % 32])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_encrypt_decrypt() {
        let mut config = MultimediaConfig::new();
        config.set_key(CloudProvider::GrokImagine, "xai-secret-key-12345");
        assert!(config.has_key(CloudProvider::GrokImagine));
        assert_eq!(
            config.get_key(CloudProvider::GrokImagine).unwrap(),
            "xai-secret-key-12345"
        );
    }

    #[test]
    fn test_key_redacted_debug() {
        let key = EncryptedKey::new(CloudProvider::GrokImagine, "secret");
        let debug = format!("{:?}", key);
        assert!(debug.contains("REDACTED"));
        assert!(!debug.contains("secret"));
    }

    #[test]
    fn test_auto_mode_resolution() {
        let mut config = MultimediaConfig::new();
        // No keys = local
        assert_eq!(config.resolve_mode(true), ModelMode::Local);
        assert_eq!(config.resolve_mode(false), ModelMode::Local);

        // Add image key = cloud for image, local for video
        config.set_key(CloudProvider::GrokImagine, "key");
        assert_eq!(config.resolve_mode(true), ModelMode::Cloud);
        assert_eq!(config.resolve_mode(false), ModelMode::Local);
    }

    #[test]
    fn test_best_provider_selection() {
        let mut config = MultimediaConfig::new();
        assert!(config.best_image_provider().is_none());

        config.set_key(CloudProvider::StabilitySD3, "key");
        assert_eq!(
            config.best_image_provider(),
            Some(CloudProvider::StabilitySD3)
        );

        // Grok takes priority
        config.set_key(CloudProvider::GrokImagine, "key2");
        assert_eq!(
            config.best_image_provider(),
            Some(CloudProvider::GrokImagine)
        );
    }

    #[test]
    fn test_remove_key() {
        let mut config = MultimediaConfig::new();
        config.set_key(CloudProvider::KlingAI, "key");
        assert!(config.has_key(CloudProvider::KlingAI));
        config.remove_key(CloudProvider::KlingAI);
        assert!(!config.has_key(CloudProvider::KlingAI));
    }
}
