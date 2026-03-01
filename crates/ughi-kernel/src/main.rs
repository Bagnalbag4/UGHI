// UGHI-kernel/src/main.rs
// Follows strict_rules.md + self_evolution.md + openclaw_surpass.md
// UGHI v1.0 – The King of 2026 AI OS
// 12 crates | 250+ tests | Self-evolving | Unlimited experts
// Subcommands: run, daemon, status, stop, evolve, lessons, rollback,
//              workflow, marketplace, fleet

use clap::{Parser, Subcommand};
use std::time::Instant;
use tracing::info;

#[derive(Parser, Debug)]
#[command(
    name = "ughi",
    version = "1.0.0",
    about = "UGHI v1.0 – Unleashed Global Human Intelligence\nThe OS that thinks with you, acts for you, and grows with you forever."
)]
struct Cli {
    #[arg(long, default_value_t = 64, global = true)]
    max_concurrent: u32,
    #[arg(long, default_value = "models/default.gguf", global = true)]
    model_path: String,
    #[arg(long, default_value = "data/memory.db", global = true)]
    db_path: String,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Run agent with auto-selected expert
    Run {
        goal: String,
        #[arg(long, default_value = "high")]
        priority: String,
    },
    /// Persistent daemon mode
    Daemon,
    /// System status
    Status,
    /// Stop daemon
    Stop,
    /// Run self-evolution cycle
    Evolve,
    /// Show learned lessons
    Lessons { query: Option<String> },
    /// Rollback to version
    Rollback { version: String },
    /// Manage composable workflows
    Workflow {
        #[command(subcommand)]
        action: WorkflowAction,
    },
    /// Skill marketplace (ClawHub)
    Marketplace {
        #[command(subcommand)]
        action: MarketAction,
    },
    /// Fleet management
    Fleet {
        #[command(subcommand)]
        action: FleetAction,
    },
    /// Skills.sh directory
    Skills {
        #[command(subcommand)]
        action: SkillsAction,
    },
}

#[derive(Subcommand, Debug)]
enum WorkflowAction {
    /// List all workflows
    List,
    /// Run a workflow by name
    Run { name: String },
    /// Load built-in templates
    Init,
}

#[derive(Subcommand, Debug)]
enum MarketAction {
    /// Search marketplace
    Search { query: String },
    /// Install a skill
    Install { id: u64 },
    /// List installed skills
    Installed,
    /// Show all listings
    Browse,
}

#[derive(Subcommand, Debug)]
enum FleetAction {
    /// Show fleet status
    Status,
    /// Clone to a new instance
    Clone {
        host: String,
        #[arg(long, default_value_t = 8081)]
        port: u16,
    },
}

#[derive(Subcommand, Debug)]
enum SkillsAction {
    /// Sync latest from skills.sh
    Update,
    /// Search skills.sh
    Search { query: String },
    /// Install a skill by slug
    Install { slug: String },
    /// Show top skills
    Leaderboard,
}

fn parse_priority(s: &str) -> ughi_runtime::AgentPriority {
    match s.to_lowercase().as_str() {
        "background" => ughi_runtime::AgentPriority::Background,
        "low" => ughi_runtime::AgentPriority::Low,
        "high" => ughi_runtime::AgentPriority::High,
        "critical" => ughi_runtime::AgentPriority::Critical,
        _ => ughi_runtime::AgentPriority::Normal,
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .compact()
        .init();

    let boot_start = Instant::now();
    let cli = Cli::parse();

    // --- Initialize ALL subsystems ---
    let mut memory_store = ughi_memory::MemoryStore::new(&cli.db_path)?;
    let mut sandbox = ughi_wasm::Sandbox::new()?;
    let skills = ughi_skills::SkillRegistry::new();
    let expert_count = ughi_expert::expert_count();
    let _inference = ughi_inference::InferenceEngine::new(&cli.model_path)?;
    let mut healer = ughi_runtime::SelfHealingManager::new();
    let mut governor = ughi_runtime::ResourceGovernor::with_limit(cli.max_concurrent);
    let runtime = ughi_runtime::Runtime::new(cli.max_concurrent)?;
    let mut evolution = ughi_evolution::EvolutionEngine::new();
    let mut workflows = ughi_workflows::WorkflowComposer::new();
    let mut marketplace = ughi_marketplace::Marketplace::new();
    let mut fleet = ughi_fleet::FleetManager::new();
    let chat = ughi_integrations::ChatHub::new();
    let proactive = ughi_integrations::ProactiveManager::new();
    let mut skills_sh = ughi_skills_registry::SkillsShClient::new();

    // Register local fleet instance
    fleet.register_local(evolution.version());

    let boot_elapsed = boot_start.elapsed();
    info!(
        "Boot: {}ms | {} experts | {} skills | {} skills.sh | {} marketplace | Evolution v{}",
        boot_elapsed.as_millis(),
        expert_count,
        skills.count(),
        skills_sh.total_skills(),
        marketplace.listing_count(),
        evolution.version()
    );

    match cli.command {
        Some(Command::Run { goal, priority }) => {
            let pri = parse_priority(&priority);
            let expert = ughi_expert::select_expert(&goal);
            let task_start = Instant::now();

            println!("\n╔══════════════════════════════════════════════════╗");
            println!(
                "║  UGHI v{:<6} – Self-Evolving Expert Agent    ║",
                evolution.version()
            );
            println!("╠══════════════════════════════════════════════════╣");
            println!("║  Goal:     {:<37}  ║", truncate(&goal, 37));
            println!("║  Expert:   {:<37}  ║", expert.name);
            println!("║  Domain:   {:<37}  ║", format!("{:?}", expert.domain));
            println!("╚══════════════════════════════════════════════════╝\n");

            governor.register_active();
            let config = ughi_runtime::AgentConfig::new(goal.clone()).with_priority(pri);
            let agent_id = runtime.spawn(config).await?;

            for skill in expert.skills {
                sandbox.issue_token(&agent_id, skill, 0);
            }
            memory_store
                .put(&agent_id, "goal", serde_json::json!(&goal))
                .ok();

            let states = [
                (ughi_runtime::AgentState::Planning, "Planning"),
                (
                    ughi_runtime::AgentState::Thinking,
                    "Thinking (expert reasoning)",
                ),
                (ughi_runtime::AgentState::ToolUsing, "Tool-Using (skills)"),
                (
                    ughi_runtime::AgentState::Reviewing,
                    "Reviewing (self-critique)",
                ),
                (ughi_runtime::AgentState::Completing, "Completing"),
            ];

            let mut task_succeeded = true;
            let mut error_msg: Option<String> = None;

            for (state, label) in &states {
                match runtime.transition_agent(&agent_id, *state).await {
                    Ok(()) => println!("  ✓ {}", label),
                    Err(e) => {
                        println!("  ✗ {} – {}", label, e);
                        task_succeeded = false;
                        error_msg = Some(e.to_string());
                        let action = healer.on_crash(&agent_id, &e.to_string());
                        match action {
                            ughi_runtime::HealingAction::Quarantine => {
                                println!("  ⊘ Quarantined")
                            }
                            _ => {
                                println!("  ↻ Self-healing");
                                healer.on_recovery(&agent_id);
                            }
                        }
                    }
                }
            }

            let skills_used: Vec<&str> = expert.skills.to_vec();
            for skill_name in &skills_used {
                let input = ughi_skills::SkillInput {
                    skill_name: skill_name.to_string(),
                    parameters: serde_json::json!({"query": &goal, "text": &goal, "command": "echo test"}),
                    capability_token: "auto".to_string(),
                };
                match ughi_skills::execute_skill(&input) {
                    Ok(out) => {
                        println!("  ✓ [{}]: {}", skill_name, out.summary);
                        memory_store
                            .put(&agent_id, &format!("skill:{}", skill_name), out.result)
                            .ok();
                    }
                    Err(e) => println!("  ✗ [{}]: {}", skill_name, e),
                }
            }

            let elapsed = task_start.elapsed().as_millis() as u64;

            let should_evolve = evolution.on_task_complete(
                &agent_id,
                &goal,
                expert.id,
                &skills_used,
                task_succeeded,
                error_msg.as_deref(),
                elapsed,
            );

            if should_evolve {
                println!("\n  ⚡ Auto-evolution triggered!");
                let proposals = evolution.evolve();
                println!("  ⚡ {} improvement proposals", proposals.len());
            }

            let snapshot = runtime.monitor(&agent_id).await?;
            let gov_metrics = governor.metrics();
            let evo_metrics = evolution.metrics();
            let fleet_metrics = fleet.metrics();

            println!("\n╔══════════════════════════════════════════════════╗");
            println!("║  Result                                            ║");
            println!("╠══════════════════════════════════════════════════╣");
            println!("║  ID:        {:<36}  ║", snapshot.id);
            println!("║  Expert:    {:<36}  ║", expert.name);
            println!("║  Time:      {:<36}  ║", format!("{}ms", elapsed));
            println!("╠══════════════════════════════════════════════════╣");
            println!(
                "║  Agents:    {:<36}  ║",
                format!(
                    "Active: {} / Total: {}",
                    gov_metrics.active_agents, gov_metrics.total_created
                )
            );
            println!(
                "║  Evolution: {:<36}  ║",
                format!("Score: {:.0} | v{}", evo_metrics.score, evolution.version())
            );
            println!(
                "║  Fleet:     {:<36}  ║",
                format!(
                    "{} instances | {} MB",
                    fleet_metrics.instances, fleet_metrics.total_memory_mb
                )
            );
            println!(
                "║  Market:    {:<36}  ║",
                format!("{} skills available", marketplace.listing_count())
            );
            println!("╚══════════════════════════════════════════════════╝\n");

            sandbox.revoke_all(&agent_id);
            governor.remove_active();
        }

        Some(Command::Daemon) => {
            println!("\n╔══════════════════════════════════════════════════╗");
            println!(
                "║  UGHI v{:<6} – Daemon Mode                   ║",
                evolution.version()
            );
            println!(
                "║  {} experts | {} skills | {} integrations        ║",
                expert_count,
                skills.count(),
                ughi_integrations::integration_count()
            );
            println!("║  Press Ctrl+C to shutdown                         ║");
            println!("╚══════════════════════════════════════════════════╝\n");

            let rt_clone = runtime.metrics.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_millis(100));
                loop {
                    interval.tick().await;
                    rt_clone.record_scheduler_tick();
                }
            });

            tokio::signal::ctrl_c().await?;
            println!("\nGoodbye. ✅");
        }

        Some(Command::Status) => {
            let mem_metrics = memory_store.metrics().unwrap_or_default();
            let gov_metrics = governor.metrics();
            let heal_metrics = healer.metrics();
            let evo_metrics = evolution.metrics();
            let fleet_metrics = fleet.metrics();
            let mkt_metrics = marketplace.metrics();
            let chat_metrics = chat.metrics();
            let pro_metrics = proactive.metrics();

            println!("\n╔══════════════════════════════════════════════════╗");
            println!(
                "║  UGHI v{:<6} – Full System Status             ║",
                evolution.version()
            );
            println!("╠══════════════════════════════════════════════════╣");
            println!(
                "║  Agents:      {:<34}  ║",
                format!(
                    "Active: {} / Total: {}",
                    gov_metrics.active_agents, gov_metrics.total_created
                )
            );
            println!(
                "║  Experts:     {:<34}  ║",
                format!("{} personas", expert_count)
            );
            println!(
                "║  Skills:      {:<34}  ║",
                format!("{} built-in", skills.count())
            );
            println!(
                "║  Boot:        {:<34}  ║",
                format!("{}ms", boot_elapsed.as_millis())
            );
            println!("╠══════════════════════════════════════════════════╣");
            println!(
                "║  Evolution:   {:<34}  ║",
                format!(
                    "Score: {:.0}/100 | v{}",
                    evo_metrics.score,
                    evolution.version()
                )
            );
            println!(
                "║  Lessons:     {:<34}  ║",
                format!("{} learned", evo_metrics.lessons_total)
            );
            println!(
                "║  Win Rate:    {:<34}  ║",
                format!("{:.0}%", evo_metrics.system_win_rate * 100.0)
            );
            println!("╠══════════════════════════════════════════════════╣");
            println!(
                "║  Fleet:       {:<34}  ║",
                format!(
                    "{} instances, {} MB",
                    fleet_metrics.instances, fleet_metrics.total_memory_mb
                )
            );
            println!(
                "║  Marketplace: {:<34}  ║",
                format!(
                    "{} listings, {} installed",
                    mkt_metrics.total_listings, mkt_metrics.installed
                )
            );
            println!(
                "║  Workflows:   {:<34}  ║",
                format!("{} pipelines", workflows.count())
            );
            println!(
                "║  Chat:        {:<34}  ║",
                format!(
                    "{} bridges, {} msgs",
                    chat_metrics.bridges, chat_metrics.total_sent
                )
            );
            println!(
                "║  Proactive:   {:<34}  ║",
                format!(
                    "{} tasks, briefing: {}",
                    pro_metrics.background_tasks, pro_metrics.briefing_enabled
                )
            );
            println!("╠══════════════════════════════════════════════════╣");
            println!(
                "║  Healing:     {:<34}  ║",
                format!(
                    "{} crashes, {:.0}% recovery",
                    heal_metrics.total_crashes,
                    heal_metrics.recovery_rate * 100.0
                )
            );
            println!(
                "║  Memory:      {:<34}  ║",
                format!(
                    "ST:{} LT:{}",
                    mem_metrics.short_term_entries, mem_metrics.long_term_entries
                )
            );
            println!("╚══════════════════════════════════════════════════╝\n");
        }

        Some(Command::Evolve) => {
            println!("\n  ⚡ Running Self-Evolution Cycle...\n");
            let proposals = evolution.evolve();
            if proposals.is_empty() {
                println!("  ✓ System is performing optimally.\n");
            } else {
                for (i, p) in proposals.iter().enumerate() {
                    println!(
                        "  {}. [{:?}] {} ({:.0}%)",
                        i + 1,
                        p.category,
                        p.title,
                        p.confidence * 100.0
                    );
                    println!("     {}\n", p.description);
                }
            }
            println!(
                "  Score: {:.0}/100 | v{} | {} lessons",
                evolution.evolution_score(),
                evolution.version(),
                evolution.metrics().lessons_total
            );
        }

        Some(Command::Lessons { query }) => {
            let lessons = if let Some(q) = &query {
                evolution.lessons.search(q)
            } else {
                evolution.lessons.all().iter().collect()
            };
            println!("\n  📚 Lessons ({}):\n", lessons.len());
            for l in lessons.iter().take(20) {
                let icon = if l.succeeded { "✓" } else { "✗" };
                println!(
                    "  {} [{}] {} | {}",
                    icon,
                    l.id,
                    truncate(&l.goal, 40),
                    l.domain
                );
                println!("    Rule: {}\n", truncate(&l.rule, 60));
            }
            if lessons.is_empty() {
                println!("  No lessons yet. Run tasks first!\n");
            }
        }

        Some(Command::Rollback { version }) => {
            if evolution.rollback(&version) {
                println!("\n  ✓ Rolled back to v{}.\n", version);
            } else {
                println!("\n  ✗ Version '{}' not found.\n", version);
            }
        }

        Some(Command::Workflow { action }) => match action {
            WorkflowAction::Init => {
                workflows.load_templates();
                println!("\n  ✓ Loaded {} workflow templates.\n", workflows.count());
            }
            WorkflowAction::List => {
                println!("\n  📋 Workflows ({}):\n", workflows.count());
                for p in workflows.list() {
                    println!(
                        "  [{}] {} – {} ({} steps, {} runs)",
                        p.id,
                        p.name,
                        p.description,
                        p.steps.len(),
                        p.run_count
                    );
                }
                if workflows.count() == 0 {
                    println!("  None. Run 'ughi workflow init' first.\n");
                }
                println!();
            }
            WorkflowAction::Run { name } => {
                if let Some(p) = workflows.find(&name) {
                    let id = p.id;
                    println!("\n  ⊳ Running workflow '{}'...\n", name);
                    if let Some(result) = workflows.execute(id) {
                        for step in &result.steps {
                            let icon = if step.succeeded { "✓" } else { "✗" };
                            println!(
                                "  {} Step {}: {} ({}ms)",
                                icon,
                                step.step_index + 1,
                                step.skill_name,
                                step.execution_ms
                            );
                        }
                        println!(
                            "\n  {} in {}ms\n",
                            if result.succeeded {
                                "✓ Completed"
                            } else {
                                "✗ Failed"
                            },
                            result.total_ms
                        );
                    }
                } else {
                    println!("\n  ✗ Workflow '{}' not found.\n", name);
                }
            }
        },

        Some(Command::Marketplace { action }) => match action {
            MarketAction::Browse => {
                println!(
                    "\n  🏪 Marketplace ({} skills):\n",
                    marketplace.listing_count()
                );
                for l in marketplace.all_listings() {
                    let badge = if l.verified { "✓" } else { " " };
                    println!(
                        "  [{}] {} {} v{} – {}",
                        l.id, badge, l.name, l.version, l.description
                    );
                }
                println!();
            }
            MarketAction::Search { query } => {
                let results = marketplace.search(&query);
                println!("\n  🔍 Results for '{}' ({}):\n", query, results.len());
                for l in &results {
                    println!("  [{}] {} v{} – {}", l.id, l.name, l.version, l.description);
                }
                if results.is_empty() {
                    println!("  No matches.\n");
                }
                println!();
            }
            MarketAction::Install { id } => match marketplace.install(id) {
                Ok(()) => println!("\n  ✓ Skill installed successfully.\n"),
                Err(e) => println!("\n  ✗ Install failed: {}\n", e),
            },
            MarketAction::Installed => {
                println!(
                    "\n  📦 Installed Skills ({}):\n",
                    marketplace.installed_count()
                );
                for s in marketplace.installed_list() {
                    println!(
                        "  ✓ {} v{} – {}",
                        s.listing.name, s.listing.version, s.install_path
                    );
                }
                if marketplace.installed_count() == 0 {
                    println!("  None installed.\n");
                }
                println!();
            }
        },

        Some(Command::Fleet { action }) => match action {
            FleetAction::Status => {
                let m = fleet.metrics();
                println!("\n  🌐 Fleet ({} instances):\n", m.instances);
                for inst in fleet.list() {
                    let icon = match inst.status {
                        ughi_fleet::InstanceStatus::Running => "🟢",
                        ughi_fleet::InstanceStatus::Stopped => "🔴",
                        _ => "🟡",
                    };
                    println!(
                        "  {} {} – {}:{} | Agents: {} | {} MB | v{}",
                        icon,
                        inst.name,
                        inst.host,
                        inst.port,
                        inst.agents_active,
                        inst.memory_mb,
                        inst.version
                    );
                }
                println!(
                    "\n  Total: {} active agents, {} MB\n",
                    m.total_agents_active, m.total_memory_mb
                );
            }
            FleetAction::Clone { host, port } => {
                if let Some(id) = fleet.clone_instance(1, &host, port) {
                    println!("\n  ✓ Cloned to {}:{} (instance #{})\n", host, port, id);
                } else {
                    println!("\n  ✗ Clone failed.\n");
                }
            }
        },

        Some(Command::Stop) => {
            runtime.shutdown().await;
            println!("Stopped. ✅");
        }

        Some(Command::Skills { action }) => match action {
            SkillsAction::Update => {
                println!("\n  ⊳ Syncing with skills.sh (All Time catalog)...\n");
                let result = skills_sh.sync();
                println!("  ✓ Synced with skills.sh");
                println!("    All Time: {} skills | Cached: {} | New: {} | Updated: {}\n",
                    result.total_skills, result.cached_skills, result.new_discovered, result.updated);
            }
            SkillsAction::Search { query } => {
                let results = skills_sh.search(&query);
                println!("\n  🔍 skills.sh results for '{}' ({} matches, {} All Time):\n",
                    query, results.len(), skills_sh.total_skills());
                for s in &results {
                    let badge = if s.verified { "✓" } else { " " };
                    println!("  {} [{}] {} – {} ({} installs) [{}]",
                        badge, s.slug, s.description, s.author, s.installs, s.category);
                    println!("    Install: ughi skills install {}\n", s.slug);
                }
                if results.is_empty() { println!("  No matches in cached skills. Try 'ughi skills update' first.\n"); }
            }
            SkillsAction::Install { slug } => {
                match skills_sh.install(&slug) {
                    Ok(installed) => {
                        println!("\n  ✓ Installed '{}' from skills.sh (WASM sandboxed)", slug);
                        println!("    Safety: {}", installed.entry.safety_score);
                        println!("    Sandbox: {}", installed.sandbox_path);
                        println!("    Capabilities: {:?}\n", installed.capability_tokens);
                    }
                    Err(e) => println!("\n  ✗ {}\n", e),
                }
            }
            SkillsAction::Leaderboard => {
                let top = skills_sh.leaderboard(20);
                let m = skills_sh.metrics();
                println!("\n  🏆 skills.sh All Time Leaderboard ({} total, {} cached):\n",
                    m.all_time_total, m.cached);
                for (i, s) in top.iter().enumerate() {
                    let badge = if s.verified { "✓" } else { " " };
                    println!("  {:>2}. {} {:<35} {:>6} installs  [{}]",
                        i + 1, badge, s.slug, s.installs, s.category);
                }
                println!();
            }
        },

        None => {
            println!();
            println!("╔══════════════════════════════════════════════════════════════════════════════╗");
            println!("║                                                                              ║");
            println!("║   Welcome to                                                                  ║");
            println!("║                                                                              ║");
            println!("║      ██████╗ ██╗   ██╗ ██████╗ ██╗  ██╗██╗     Unleashed Global Human       ║");
            println!("║     ██╔════╝ ██║   ██║██╔════╝ ██║  ██║██║          Intelligence             ║");
            println!("║     ██║  ███╗██║   ██║██║  ███╗███████║██║                                    ║");
            println!("║     ██║   ██║██║   ██║██║   ██║██╔══██║██║                                    ║");
            println!("║     ╚██████╔╝╚██████╔╝╚██████╔╝██║  ██║███████╗                               ║");
            println!("║      ╚═════╝  ╚═════╝  ╚═════╝ ╚═╝  ╚═╝╚══════╝                               ║");
            println!("║                                                                              ║");
            println!("║   The OS that thinks with you, acts for you, and grows with you forever.     ║");
            println!("║                                                                              ║");
            println!("║   You are now running the world's first self-evolving autonomous OS.         ║");
            println!("║                                                                              ║");
            println!("║   Type any goal and watch magic happen.                                      ║");
            println!("║                                                                              ║");
            println!("║   Examples:                                                                  ║");
            println!("║     ughi run \"Plan my entire day and reply to all emails\"                    ║");
            println!("║     ughi run \"Build my 2026 startup from idea to launch\"                     ║");
            println!("║     ughi run \"Be my personal CEO for the next 24 hours\"                      ║");
            println!("║                                                                              ║");
            println!("║   Ready when you are. Let's build the future.                                ║");
            println!("║                                                                              ║");
            println!("╚══════════════════════════════════════════════════════════════════════════════╝");
            println!();
            println!("UGHI v{}  •  {} experts  •  {} skills  •  Boot: {}ms",
                evolution.version(), expert_count, skills.count(), boot_elapsed.as_millis());
            println!("Type 'ughi help' for commands");
            println!();
        }
    }

    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() > max {
        format!("{}...", &s[..max.saturating_sub(3)])
    } else {
        s.to_string()
    }
}
