# Agenticos Skill Registry (v1.0) – World-First Lightweight Skill System

Every skill = Rust core + Go wrapper + Python SDK + WASM interface.

## Built-in Skills (must implement in Phase 1)
1. BrowserControl – Ferrum/Playwright Rust, <180 MB for 8 tabs
2. CodeExecutor – Safe Rust sandbox (wasmtime), Python subset only
3. WebSearch – Local cache + DuckDuckGo API fallback
4. FileSystem – Virtual FS with capability tokens
5. MemoryReadWrite – Vector + SQLite
6. Email/Slack/Discord – API only (no IMAP)
7. Scheduler – Cron + predictive wake
8. SelfCritique – Calls same SLM with reflection prompt
9. CollaborationVote – Multi-agent consensus
10. TerminalCommand – SSH-safe subset
11. EasyInstall – Tauri-based App with One-Click installer & Beautiful WebUI

## Skill Format (mandatory)
- Rust: trait Skill { async fn execute(&self, input: SkillInput) -> SkillOutput; }
- Memory budget per skill call: ≤ 45 MB
- Latency SLA: < 420 ms cold, < 80 ms hot
- All skills must return structured JSON + natural language summary

New skills added only via PR + benchmark proof.
