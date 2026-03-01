# AGENTICOS STRICT RULES – NON-NEGOTIABLE (Violate = Delete Repo)

1. Total RAM for 20 concurrent agents + orchestrator + SLM: ≤ 3.2 GB peak (tested on 4GB VPS)
2. Single binary size (Rust core + Go): ≤ 18 MB
3. No GPU ever. Only CPU (llama.cpp + Candle)
4. No Python in hot path – only for user-defined high-level agents
5. Every allocation must be tracked. No Arc<Mutex> spam.
6. All errors must be recoverable. No panic! in core.
7. Naming: snake_case Rust, camelCase Go, AgentXxx Python
8. Every function documented with /// + memory cost comment
9. Benchmarks must run on every commit (2GB VPS simulation)
10. Security: Zero trust. Every agent starts with zero capabilities.

This file is the constitution. AI must reference it in every response.
