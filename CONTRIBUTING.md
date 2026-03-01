# Contributing to UGHI 🚀

Thank you for your interest in contributing to UGHI! We want to make the world's most accessible, billion-scale agentic AI OS. Every contribution counts, whether it's fixing a typo, adding a new expert persona, writing a skill, or optimizing the Rust kernel.

## Getting Started

1. **Fork the repo** and clone it locally.
2. **Install dependencies:** You need `rustup`, `go`, and `node/pnpm` installed.
3. Run `make ci` to ensure everything compiles before pushing!

## Pull Requests

1. **Branch naming:** Use `feature/your-feature`, `bugfix/issue-description`.
2. **Tests:** All PRs must pass the test suite and include new tests if you added logic.
3. **Memory Limits:** DO NOT Exceed our rigid RAM memory constraints per the `strict_rules.md`. We run a 2GB VPS memory simulation in CI.

## ⚠️ Non-Negotiable Rules

Please read our `claude.md` and `strict_rules.md` files located in the root. **Any PR that violates these core laws (e.g., adding heavy Python in hot paths, breaking capability tokens, bypassing the memory limits) will be rejected automatically.**
