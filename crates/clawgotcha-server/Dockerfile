# Build from repository root: docker build -f crates/clawgotcha-server/Dockerfile .
FROM rust:1-bookworm AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo build --release -p clawgotcha-server

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/clawgotcha-server /usr/local/bin/clawgotcha-server
ENV CLAWGOTCHA_SERVER_PORT=9847
EXPOSE 9847
ENTRYPOINT ["/usr/local/bin/clawgotcha-server"]
