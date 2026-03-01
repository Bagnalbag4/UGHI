// UGHI-integrations/src/chat.rs
// Chat bridges: WhatsApp, Telegram, Discord, Slack, Signal, Matrix, iMessage
// All via webhook/API – no heavy SDKs. Memory: ~1 KB per bridge.
// H-01 FIX: API tokens wrapped in Secret<String> (redacted in Debug/Serialize)
// H-03 FIX: Inbox/outbox capped at MAX_QUEUE_SIZE (1024) to prevent DoS

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

/// Maximum inbox/outbox size. Prevents unbounded memory growth (H-03 fix).
const MAX_QUEUE_SIZE: usize = 1024;

/// Secret wrapper: redacts value in Debug, Display, and Serialize.
/// The inner value is NEVER logged, printed, or serialized.
/// Memory cost: same as String (~24 bytes + heap)
#[derive(Clone)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: &str) -> Self {
        Self(value.to_string())
    }
    /// Access the inner value (explicit, auditable call site).
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Secret([REDACTED])")
    }
}

impl std::fmt::Display for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[REDACTED]")
    }
}

impl Serialize for Secret {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str("[REDACTED]")
    }
}

impl<'de> Deserialize<'de> for Secret {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let val = String::deserialize(d)?;
        Ok(Secret(val))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChatPlatform {
    WhatsApp,
    Telegram,
    Discord,
    Slack,
    Signal,
    Matrix,
    IMessage,
}

impl std::fmt::Display for ChatPlatform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WhatsApp => write!(f, "WhatsApp"),
            Self::Telegram => write!(f, "Telegram"),
            Self::Discord => write!(f, "Discord"),
            Self::Slack => write!(f, "Slack"),
            Self::Signal => write!(f, "Signal"),
            Self::Matrix => write!(f, "Matrix"),
            Self::IMessage => write!(f, "iMessage"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatBridge {
    pub platform: ChatPlatform,
    pub webhook_url: String,
    /// H-01 FIX: API token wrapped in Secret — never logged or serialized.
    pub api_token: Secret,
    pub enabled: bool,
    pub messages_sent: u64,
    pub messages_received: u64,
}

impl ChatBridge {
    pub fn new(platform: ChatPlatform, webhook_url: &str, api_token: &str) -> Self {
        Self {
            platform,
            webhook_url: webhook_url.to_string(),
            api_token: Secret::new(api_token),
            enabled: true,
            messages_sent: 0,
            messages_received: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub platform: ChatPlatform,
    pub sender: String,
    pub content: String,
    pub timestamp_ms: u64,
    pub is_command: bool,
}

/// Unified chat hub managing all platform bridges.
pub struct ChatHub {
    bridges: Vec<ChatBridge>,
    inbox: Vec<ChatMessage>,
    outbox: Vec<ChatMessage>,
}

impl ChatHub {
    pub fn new() -> Self {
        Self {
            bridges: Vec::with_capacity(7),
            inbox: Vec::with_capacity(64),
            outbox: Vec::with_capacity(64),
        }
    }

    /// Register a chat platform bridge.
    pub fn register(&mut self, bridge: ChatBridge) {
        info!(platform = %bridge.platform, "chat bridge registered");
        self.bridges.push(bridge);
    }

    /// Send a message to a platform.
    pub fn send(&mut self, platform: ChatPlatform, content: &str) -> Result<(), String> {
        let bridge = self
            .bridges
            .iter_mut()
            .find(|b| b.platform == platform && b.enabled)
            .ok_or_else(|| format!("{} bridge not configured", platform))?;

        bridge.messages_sent += 1;
        // H-03 FIX: evict oldest if at capacity
        if self.outbox.len() >= MAX_QUEUE_SIZE {
            warn!("outbox at capacity ({}), evicting oldest", MAX_QUEUE_SIZE);
            self.outbox.drain(0..MAX_QUEUE_SIZE / 4);
        }

        self.outbox.push(ChatMessage {
            platform,
            sender: "UGHI".to_string(),
            content: content.to_string(),
            timestamp_ms: current_time_ms(),
            is_command: false,
        });

        info!(platform = %platform, "message sent");
        Ok(())
    }

    /// Receive/queue a message from a platform.
    pub fn receive(&mut self, platform: ChatPlatform, sender: &str, content: &str) {
        let is_command = content.starts_with('/') || content.starts_with("UGHI ");

        if let Some(bridge) = self.bridges.iter_mut().find(|b| b.platform == platform) {
            bridge.messages_received += 1;
        }

        // H-03 FIX: evict oldest if at capacity
        if self.inbox.len() >= MAX_QUEUE_SIZE {
            warn!("inbox at capacity ({}), evicting oldest", MAX_QUEUE_SIZE);
            self.inbox.drain(0..MAX_QUEUE_SIZE / 4);
        }

        self.inbox.push(ChatMessage {
            platform,
            sender: sender.to_string(),
            content: content.to_string(),
            timestamp_ms: current_time_ms(),
            is_command,
        });
    }

    /// Get pending commands from inbox.
    pub fn pending_commands(&self) -> Vec<&ChatMessage> {
        self.inbox.iter().filter(|m| m.is_command).collect()
    }

    /// Get all registered platforms.
    pub fn platforms(&self) -> Vec<ChatPlatform> {
        self.bridges.iter().map(|b| b.platform).collect()
    }

    /// Broadcast to all enabled platforms.
    pub fn broadcast(&mut self, content: &str) {
        let platforms: Vec<ChatPlatform> = self
            .bridges
            .iter()
            .filter(|b| b.enabled)
            .map(|b| b.platform)
            .collect();

        for p in platforms {
            let _ = self.send(p, content);
        }
    }

    pub fn bridge_count(&self) -> usize {
        self.bridges.len()
    }
    pub fn inbox_count(&self) -> usize {
        self.inbox.len()
    }
    pub fn outbox_count(&self) -> usize {
        self.outbox.len()
    }

    pub fn metrics(&self) -> ChatMetrics {
        ChatMetrics {
            bridges: self.bridges.len() as u32,
            total_sent: self.bridges.iter().map(|b| b.messages_sent).sum(),
            total_received: self.bridges.iter().map(|b| b.messages_received).sum(),
            pending_commands: self.pending_commands().len() as u32,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChatMetrics {
    pub bridges: u32,
    pub total_sent: u64,
    pub total_received: u64,
    pub pending_commands: u32,
}

fn current_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_bridge() {
        let mut hub = ChatHub::new();
        hub.register(ChatBridge::new(
            ChatPlatform::Telegram,
            "https://t.me/hook",
            "tok",
        ));
        assert_eq!(hub.bridge_count(), 1);
    }

    #[test]
    fn test_send_message() {
        let mut hub = ChatHub::new();
        hub.register(ChatBridge::new(ChatPlatform::WhatsApp, "url", "tok"));
        assert!(hub.send(ChatPlatform::WhatsApp, "Hello!").is_ok());
        assert_eq!(hub.outbox_count(), 1);
    }

    #[test]
    fn test_receive_command() {
        let mut hub = ChatHub::new();
        hub.register(ChatBridge::new(ChatPlatform::Discord, "url", "tok"));
        hub.receive(ChatPlatform::Discord, "user1", "/status");
        hub.receive(ChatPlatform::Discord, "user2", "just chatting");
        assert_eq!(hub.pending_commands().len(), 1);
    }

    #[test]
    fn test_broadcast() {
        let mut hub = ChatHub::new();
        hub.register(ChatBridge::new(ChatPlatform::Telegram, "url", "tok"));
        hub.register(ChatBridge::new(ChatPlatform::Slack, "url", "tok"));
        hub.broadcast("Daily briefing!");
        assert_eq!(hub.outbox_count(), 2);
    }

    #[test]
    fn test_unregistered_platform() {
        let mut hub = ChatHub::new();
        assert!(hub.send(ChatPlatform::Signal, "test").is_err());
    }
}
