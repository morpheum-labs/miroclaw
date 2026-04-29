# syntax=docker/dockerfile:1.7
# Miroclaw multi-stage image (web UI + zeroclawlabs binary `miroclaw`).

# ── Stage 0: Frontend build ─────────────────────────────────────
FROM oven/bun:1.3-alpine AS web-builder
WORKDIR /web
COPY web/package.json web/bun.lock ./
RUN bun install --frozen-lockfile
COPY web/ .
RUN bun run build

# ── Stage 1: Build ────────────────────────────────────────────
FROM rust:1.94-slim@sha256:da9dab7a6b8dd428e71718402e97207bb3e54167d37b5708616050b1e8f60ed6 AS builder

WORKDIR /app
ARG MIROCLAW_CARGO_FEATURES="memory-postgres,channel-lark"

# Install build dependencies
RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
  --mount=type=cache,target=/var/lib/apt,sharing=locked \
  apt-get update && apt-get install -y \
  pkg-config \
  && rm -rf /var/lib/apt/lists/*

# 1. Copy manifests to cache dependencies
COPY Cargo.toml Cargo.lock ./
# Include every workspace member: Cargo.lock is generated for the full workspace.
# Previously we used sed to drop `crates/robot-kit`, which made the manifest disagree
# with the lockfile and caused `cargo --locked` to fail (Cargo refused to rewrite the lock).
COPY crates/robot-kit/ crates/robot-kit/
COPY crates/aardvark-sys/ crates/aardvark-sys/
COPY apps/tauri/ apps/tauri/
# Create dummy targets declared in Cargo.toml so manifest parsing succeeds.
RUN mkdir -p src benches \
  && echo "fn main() {}" > src/main.rs \
  && echo "" > src/lib.rs \
  && echo "fn main() {}" > benches/agent_benchmarks.rs
RUN --mount=type=cache,id=miroclaw-cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
  --mount=type=cache,id=miroclaw-cargo-git,target=/usr/local/cargo/git,sharing=locked \
  --mount=type=cache,id=miroclaw-target,target=/app/target,sharing=locked \
  if [ -n "$MIROCLAW_CARGO_FEATURES" ]; then \
  cargo build --release --locked -p zeroclawlabs --features "$MIROCLAW_CARGO_FEATURES"; \
  else \
  cargo build --release --locked -p zeroclawlabs; \
  fi
RUN rm -rf src benches

# 2. Copy only build-relevant source paths (avoid cache-busting on docs/tests/scripts)
COPY src/ src/
COPY benches/ benches/
COPY --from=web-builder /web/dist web/dist
COPY *.rs .
RUN touch src/main.rs
RUN --mount=type=cache,id=miroclaw-cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
  --mount=type=cache,id=miroclaw-cargo-git,target=/usr/local/cargo/git,sharing=locked \
  --mount=type=cache,id=miroclaw-target,target=/app/target,sharing=locked \
  rm -rf target/release/.fingerprint/zeroclawlabs-* \
  target/release/deps/zeroclawlabs-* \
  target/release/incremental/zeroclawlabs-* && \
  if [ -n "$MIROCLAW_CARGO_FEATURES" ]; then \
  cargo build --release --locked -p zeroclawlabs --features "$MIROCLAW_CARGO_FEATURES"; \
  else \
  cargo build --release --locked -p zeroclawlabs; \
  fi && \
  cp target/release/miroclaw /app/miroclaw && \
  strip /app/miroclaw
RUN size=$(stat -c%s /app/miroclaw) && \
  if [ "$size" -lt 1000000 ]; then echo "ERROR: binary too small (${size} bytes), likely dummy build artifact" && exit 1; fi

# Prepare runtime directory structure and default config inline (no extra stage)
RUN mkdir -p /miroclaw-data/.miroclaw /miroclaw-data/workspace && \
  printf '%s\n' \
  'workspace_dir = "/miroclaw-data/workspace"' \
  'config_path = "/miroclaw-data/.miroclaw/config.toml"' \
  'api_key = ""' \
  'default_provider = "openrouter"' \
  'default_model = "anthropic/claude-sonnet-4-20250514"' \
  'default_temperature = 0.7' \
  '' \
  '[gateway]' \
  'port = 42617' \
  'host = "[::]"' \
  'allow_public_bind = true' \
  '' \
  '[autonomy]' \
  'level = "supervised"' \
  'auto_approve = ["file_read", "file_write", "file_edit", "memory_recall", "memory_store", "web_search_tool", "web_fetch", "calculator", "glob_search", "content_search", "image_info", "weather", "git_operations"]' \
  > /miroclaw-data/.miroclaw/config.toml && \
  chown -R 1000:1000 /miroclaw-data

# ── Stage 2: Development Runtime (Debian) ────────────────────
FROM debian:trixie-slim@sha256:f6e2cfac5cf956ea044b4bd75e6397b4372ad88fe00908045e9a0d21712ae3ba AS dev

# Install essential runtime dependencies only (use docker-compose.override.yml for dev tools)
RUN apt-get update && apt-get install -y \
  ca-certificates \
  curl \
  && rm -rf /var/lib/apt/lists/*

COPY --from=builder /miroclaw-data /miroclaw-data
COPY --from=builder /app/miroclaw /usr/local/bin/miroclaw

# Overwrite minimal config with DEV template (Ollama defaults)
COPY dev/config.template.toml /miroclaw-data/.miroclaw/config.toml
RUN chown 1000:1000 /miroclaw-data/.miroclaw/config.toml

# Environment setup
# Ensure UTF-8 locale so CJK / multibyte input is handled correctly
ENV LANG=C.UTF-8
# Use consistent workspace path
ENV MIROCLAW_WORKSPACE=/miroclaw-data/workspace
ENV HOME=/miroclaw-data
# Defaults for local dev (Ollama) - matches config.template.toml
ENV PROVIDER="ollama"
ENV MIROCLAW_MODEL="llama3.2"
ENV MIROCLAW_GATEWAY_PORT=42617

# Note: API_KEY is intentionally NOT set here to avoid confusion.
# It is set in config.toml as the Ollama URL.

WORKDIR /miroclaw-data
USER 1000:1000
EXPOSE 42617
HEALTHCHECK --interval=60s --timeout=10s --retries=3 --start-period=10s \
  CMD ["miroclaw", "status", "--format=exit-code"]
ENTRYPOINT ["miroclaw"]
CMD ["daemon"]

# ── Stage 3: Production Runtime (Distroless) ─────────────────
FROM gcr.io/distroless/cc-debian13:nonroot@sha256:84fcd3c223b144b0cb6edc5ecc75641819842a9679a3a58fd6294bec47532bf7 AS release

COPY --from=builder /app/miroclaw /usr/local/bin/miroclaw
COPY --from=builder /miroclaw-data /miroclaw-data

# Environment setup
# Ensure UTF-8 locale so CJK / multibyte input is handled correctly
ENV LANG=C.UTF-8
ENV MIROCLAW_WORKSPACE=/miroclaw-data/workspace
ENV HOME=/miroclaw-data
# Default provider and model are set in config.toml, not here,
# so config file edits are not silently overridden
#ENV PROVIDER=
ENV MIROCLAW_GATEWAY_PORT=42617

# API_KEY must be provided at runtime!

WORKDIR /miroclaw-data
USER 1000:1000
EXPOSE 42617
HEALTHCHECK --interval=60s --timeout=10s --retries=3 --start-period=10s \
  CMD ["miroclaw", "status", "--format=exit-code"]
ENTRYPOINT ["miroclaw"]
CMD ["daemon"]
