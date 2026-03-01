# 🚀 UGHI: One-Click Desktop App (Easy Use Guide)

UGHI is now available for absolutely **anyone** to use, no terminal required. With our new Tauri-based Desktop App, installing and running UGHI is as simple as a single click.

## Features Included 🌟
- **True One-Click Installer**: `.exe` for Windows, `.dmg` for Mac, `.AppImage`/`.deb` for Linux.
- **Beautiful WebUI**: Inspired by clean, modern applications with a simple "Type your goal" interface.
- **Auto-Onboarding**: The first time you launch UGHI, you are welcomed by a setup wizard (no setup needed, just enter your name!).
- **Workspace Sidebar**: Real-time memory usage, active agents counter, and direct access to Skills, News, Media, and Computer Mode.
- **Tray Icon & Auto-Start**: UGHI runs quietly in the background, always ready when you need it.
- **Under the Hood**: It still utilizes the lightning-fast Rust Micro-Kernel and Go Orchestrator within our strict memory budgets (<140MB per agent).

## How to Install 💻

1. Go to our GitHub Releases page or the Homepage.
2. Download the installer for your OS:
   - **Windows:** `UGHI-Setup.exe`
   - **Mac:** `UGHI.dmg`
   - **Linux:** `UGHI.AppImage`
3. Open the file.
4. **Done!** The UGHI app will automatically launch.

## Using UGHI 🧠
1. Open the UGHI application.
2. In the center search box, type your goal (e.g., _"Build a React dashboard"_ or _"Research the latest news on Quantum Computing"_).
3. Click the Run button (or press Enter).
4. Watch the Live Agent Tree dynamically spawn world-class experts and accomplish your tasks!

*(For advanced users, the CLI fallback `ughi run ...` is still fully supported and available natively after installation).*

## Chat Integrations (Telegram, Discord, Slack) 💬

UGHI v0.1.0 supports listening to your chat apps natively! (No Python required).

1. **Telegram:** Open BotFather on Telegram, create a bot, and set its webhook URL to `http://<your-ip>:8080/api/webhooks/telegram`
2. **Discord:** Go to the Discord Developer Portal, create an App, and set the Interactions Endpoint URL to `http://<your-ip>:8080/api/webhooks/discord`
3. **Slack:** Go to Slack API, create an App, enable Event Subscriptions, and set the Request URL to `http://<your-ip>:8080/api/webhooks/slack`

*When someone messages the bot or uses `/ughi`, the Orchestrator will automatically spawn an agent to answer them!*
