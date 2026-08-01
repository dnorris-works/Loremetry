# Multi-stage: Vue SPA + Rust Axum binary (portable container image)

FROM node:22-bookworm AS ui
WORKDIR /ui
COPY ui/package.json ui/package-lock.json ./
RUN npm ci
COPY ui/ ./
RUN npm run build

FROM rust:1.86-bookworm AS rust
WORKDIR /build
RUN apt-get update && apt-get install -y --no-install-recommends pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo build --release -p loremetry-web -p loremetry-worker \
    && cp target/release/loremetry-web /build/loremetry-web \
    && cp target/release/loremetry-worker /build/loremetry-worker

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates libssl3 pandoc \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=rust /build/loremetry-web /app/loremetry-web
COPY --from=rust /build/loremetry-worker /app/loremetry-worker
COPY scripts/start.sh /app/start.sh
COPY --from=ui /ui/dist /app/ui/dist
ENV PORT=8080
ENV STATIC_DIR=/app/ui/dist
ENV RUST_LOG=info
EXPOSE 8080
RUN chmod +x /app/start.sh
CMD ["/app/start.sh"]
