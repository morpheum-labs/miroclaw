# Miroclaw / miroclawlabs gateway image (multi-stage). Build from repo root.
FROM rust:1-bookworm AS builder
WORKDIR /app
COPY . .
RUN cargo build --release --bin miroclaw

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/miroclaw /usr/local/bin/miroclaw
WORKDIR /workspace
ENV MIROCLAW_WORKSPACE=/workspace
ENTRYPOINT ["/usr/local/bin/miroclaw"]
