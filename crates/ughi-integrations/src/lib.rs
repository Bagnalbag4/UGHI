// UGHI-integrations/src/lib.rs
pub mod chat;
pub mod proactive;

pub use chat::{ChatBridge, ChatHub, ChatMessage, ChatMetrics, ChatPlatform};
pub use proactive::{ProactiveManager, ProactiveMetrics, BackgroundTask, BriefingSection, DailyBriefing};

pub fn integration_count() -> usize { 7 } // 7 chat platforms

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_integration_count() {
        assert_eq!(integration_count(), 7);
    }
}
