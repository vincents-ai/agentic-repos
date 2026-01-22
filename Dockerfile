FROM rust:1.92-slim as builder

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    git \
    && rm -rf /var/lib/apt/lists/*

RUN useradd -ms /bin/bash appuser
WORKDIR /app

COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release && rm -rf src target/release/deps/repos_mcp*

COPY src ./src
RUN cargo build --release

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    git \
    curl \
    && rm -rf /var/lib/apt/lists/*

RUN useradd -r -u 1000 -s /bin/false appuser

RUN mkdir -p /app/data /app/config /app/.ssh && \
    chown -R appuser:appuser /app

WORKDIR /app

COPY --from=builder /app/target/release/repos-mcp /usr/local/bin/repos-mcp

COPY config-example.yaml /app/config/config.yaml

USER appuser

EXPOSE 2222 8090

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8090/api/v1/health || exit 1

CMD ["repos-mcp", "--config", "/app/config/config.yaml"]