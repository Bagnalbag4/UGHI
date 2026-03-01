# UGHI v1.0 Production Build
# Follows strict_rules.md | Binary ≤ 25 MB | Idle RAM ≤ 380 MB
# Single binary: Rust kernel + inference + memory + wasm + skills

.PHONY: all build test check clean docker release

# Default target
all: check test build

# Type check all crates
check:
	cargo check --workspace

# Run all tests
test:
	cargo test --workspace

# Debug build
build:
	cargo build --workspace

# Release build (optimized, stripped)
release:
	cargo build --release --workspace
	@echo "Binary: target/release/UGHI"
	@ls -la target/release/UGHI* 2>/dev/null || dir target\release\UGHI* 2>nul

# Clean build artifacts
clean:
	cargo clean

# Build Go orchestrator
orchestrator:
	cd orchestrator && go build -ldflags="-s -w" -o ughi-orchestrator .

# Docker build (2GB VPS optimized)
docker:
	docker build -t UGHI:latest .

# Docker compose (full system)
docker-up:
	docker-compose up -d

docker-down:
	docker-compose down

# Run format + clippy
lint:
	cargo fmt --all -- --check
	cargo clippy --workspace -- -D warnings

# Quick smoke test
smoke:
	cargo run -- --version
	cargo run -- run "Hello UGHI"

# Full CI pipeline
ci: check lint test release
	@echo "CI pipeline complete"
