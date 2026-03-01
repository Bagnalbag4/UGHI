# Agenticos Agent Specification v1.0 (Unbreakable Standard)

## Core Philosophy
An Agent in Agenticos is NOT a Python script. It is a first-class, memory-safe, sandboxed OS process equivalent – lightweight, immortal until killed, and capable of true collaboration.

## Agent Lifecycle (States)
1. Spawned (idle < 80ms)
2. Planning
3. Tool-Using
4. Thinking (SLM inference)
5. Collaborating
6. Reviewing (self-critique)
7. Completing / Suspending
8. Crashed → Auto-Recovered

## Memory Model (per agent – HARD LIMIT)
- Short-term: 40 MB (in-memory)
- Long-term: Shared SQLite + vector (max 15 MB per agent)
- Total per agent peak: 140 MB (including model KV cache sharing)

## Communication
- Go channels + typed protobuf messages (zero-copy)
- Supervisor-Worker tree only (no full mesh unless requested)

## Identity
Every agent has unique 12-char ID, parent chain, goal vector, and capability manifest.

## Security
Zero filesystem access outside assigned sandbox. All tools via WASM capability tokens.

This file is LAW. Any deviation = project failure.
