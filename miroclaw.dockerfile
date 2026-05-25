# syntax=docker/dockerfile:1.7
# Miroclaw multi-stage image (`miroclaw` binary).
#
# Runtime layout (hub / multi-agent):
#   /miroclaw-data/.miroclaw/config.toml      — hub supervisor + public gateway
#   /miroclaw-data/.miroclaw/registry.toml    — agent registry
#   /miroclaw-data/.miroclaw/profiles/<name>/ — per-agent config.toml + workspace/
#
# Default CMD runs `miroclaw daemon` in hub mode with a `main` profile on internal port 18080.
# Mount /miroclaw-data/.miroclaw to persist config, registry, profiles, and paired tokens.
#
# This image does not build Bun/Vite; use `[webui].external_path` (or `MIROCLAW_WEBUI_EXTERNAL_PATH`)
# to serve a built `dist/` from disk, or disable the dashboard with `[webui].disabled`.

# ── Stage 0: Rust build ───────────────────────────────────────
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
COPY crates/aardvark-sys/ crates/aardvark-sys/
COPY crates/clawgotcha/ crates/clawgotcha/
# Create dummy targets declared in Cargo.toml so manifest parsing succeeds.
RUN mkdir -p src benches \
  && echo "fn main() {}" > src/main.rs \
  && echo "" > src/lib.rs \
  && echo "fn main() {}" > benches/agent_benchmarks.rs
RUN --mount=type=cache,id=miroclaw-cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
  --mount=type=cache,id=miroclaw-cargo-git,target=/usr/local/cargo/git,sharing=locked \
  --mount=type=cache,id=miroclaw-target,target=/app/target,sharing=locked \
  if [ -n "$MIROCLAW_CARGO_FEATURES" ]; then \
  cargo build --release --locked -p miroclawlabs --features "$MIROCLAW_CARGO_FEATURES"; \
  else \
  cargo build --release --locked -p miroclawlabs; \
  fi
RUN rm -rf src benches

# 2. Copy only build-relevant source paths (avoid cache-busting on docs/tests/scripts)
COPY src/ src/
COPY *.rs .
RUN touch src/main.rs
RUN --mount=type=cache,id=miroclaw-cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
  --mount=type=cache,id=miroclaw-cargo-git,target=/usr/local/cargo/git,sharing=locked \
  --mount=type=cache,id=miroclaw-target,target=/app/target,sharing=locked \
  rm -rf target/release/.fingerprint/miroclawlabs-* \
  target/release/deps/miroclawlabs-* \
  target/release/incremental/miroclawlabs-* && \
  if [ -n "$MIROCLAW_CARGO_FEATURES" ]; then \
  cargo build --release --locked -p miroclawlabs --features "$MIROCLAW_CARGO_FEATURES"; \
  else \
  cargo build --release --locked -p miroclawlabs; \
  fi && \
  cp target/release/miroclaw /app/miroclaw && \
  strip /app/miroclaw
RUN size=$(stat -c%s /app/miroclaw) && \
  if [ "$size" -lt 1000000 ]; then echo "ERROR: binary too small (${size} bytes), likely dummy build artifact" && exit 1; fi

# Seed hub + agent profile layout (see docker/ for editable templates)
COPY docker/hub.config.toml docker/registry.toml /docker-seed/
COPY docker/profiles/ /docker-seed/profiles/
RUN mkdir -p /miroclaw-data/.miroclaw/profiles/main/workspace && \
  cp /docker-seed/hub.config.toml /miroclaw-data/.miroclaw/config.toml && \
  cp /docker-seed/registry.toml /miroclaw-data/.miroclaw/registry.toml && \
  cp /docker-seed/profiles/main.config.toml /miroclaw-data/.miroclaw/profiles/main/config.toml && \
  cp /docker-seed/profiles/IDENTITY.md /docker-seed/profiles/SOUL.md \
    /miroclaw-data/.miroclaw/profiles/main/workspace/ && \
  chown -R 1000:1000 /miroclaw-data

# ── Stage 1: Development Runtime (Debian) ───────────────────
FROM debian:trixie-slim@sha256:f6e2cfac5cf956ea044b4bd75e6397b4372ad88fe00908045e9a0d21712ae3ba AS dev

# Install essential runtime dependencies only (use docker-compose.override.yml for dev tools)
RUN apt-get update && apt-get install -y \
  ca-certificates \
  curl \
  wget \
  && rm -rf /var/lib/apt/lists/*

COPY --from=builder /miroclaw-data /miroclaw-data
COPY --from=builder /app/miroclaw /usr/local/bin/miroclaw

# Dev overrides: Ollama defaults on the main profile (hub layout unchanged)
COPY docker/dev/hub.config.toml /miroclaw-data/.miroclaw/config.toml
COPY docker/dev/profiles/main.config.toml /miroclaw-data/.miroclaw/profiles/main/config.toml
RUN chown -R 1000:1000 /miroclaw-data/.miroclaw

# Environment setup
# Ensure UTF-8 locale so CJK / multibyte input is handled correctly
ENV LANG=C.UTF-8
ENV HOME=/miroclaw-data
# Defaults for local dev (Ollama) — profile config holds the Ollama base URL
ENV PROVIDER="ollama"
ENV MIROCLAW_MODEL="llama3.2"
ENV MIROCLAW_GATEWAY_PORT=42617

WORKDIR /miroclaw-data
USER 1000:1000
EXPOSE 42617
HEALTHCHECK --interval=60s --timeout=10s --retries=3 --start-period=10s \
  CMD ["miroclaw", "status", "--format=exit-code"]
ENTRYPOINT ["miroclaw"]
CMD ["daemon"]

# ── Stage 2: Production Runtime (Debian) ─────────────────────
# curl/wget for health/debug (e.g. clawgotcha reachability from inside the container).
FROM debian:trixie-slim@sha256:f6e2cfac5cf956ea044b4bd75e6397b4372ad88fe00908045e9a0d21712ae3ba AS release

RUN apt-get update && apt-get install -y \
  ca-certificates \
  curl \
  wget \
  && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/miroclaw /usr/local/bin/miroclaw
COPY --from=builder /miroclaw-data /miroclaw-data

# Environment setup
# Ensure UTF-8 locale so CJK / multibyte input is handled correctly
ENV LANG=C.UTF-8
ENV HOME=/miroclaw-data
# Provider/model live in profiles/main/config.toml — set API_KEY or mount config at runtime
ENV MIROCLAW_GATEWAY_PORT=42617

# API_KEY must be provided at runtime (or edit profiles/main/config.toml via a mounted volume)!

WORKDIR /miroclaw-data
USER 1000:1000
EXPOSE 42617
HEALTHCHECK --interval=60s --timeout=10s --retries=3 --start-period=10s \
  CMD ["miroclaw", "status", "--format=exit-code"]
ENTRYPOINT ["miroclaw"]
CMD ["daemon"]
