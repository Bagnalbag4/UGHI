# UGHI v1.0 – Multi-stage Docker build
# Follows strict_rules.md | Binary ≤ 25 MB | Optimized for 2-4GB VPS
# Stage 1: Build Rust kernel (static MUSL for small binary)
# Stage 2: Build Go orchestrator
# Stage 3: Runtime image (~50 MB total)

# --- Stage 1: Rust Builder ---
FROM rust:1.82-bookworm AS rust-builder
WORKDIR /build

# Copy workspace
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/

# Build release binary
RUN cargo build --release --workspace \
    && strip target/release/UGHI 2>/dev/null || true

# --- Stage 2: Go Builder ---
FROM golang:1.22-bookworm AS go-builder
WORKDIR /build

COPY orchestrator/ orchestrator/
WORKDIR /build/orchestrator
RUN go mod download \
    && CGO_ENABLED=0 go build -ldflags="-s -w" -o ughi-orchestrator .

# --- Stage 3: Runtime ---
FROM debian:bookworm-slim AS runtime

# Minimal runtime deps
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates sqlite3 \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -m -s /bin/bash UGHI
USER UGHI
WORKDIR /home/UGHI

# Copy binaries
COPY --from=rust-builder /build/target/release/UGHI ./UGHI
COPY --from=go-builder /build/orchestrator/ughi-orchestrator ./ughi-orchestrator

# Create data directories
RUN mkdir -p data models tmp

# Default port
EXPOSE 8420

# Health check
HEALTHCHECK --interval=30s --timeout=5s \
    CMD ./UGHI --version || exit 1

# Default: run in daemon mode
ENTRYPOINT ["./UGHI"]
CMD ["daemon"]
