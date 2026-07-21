# Multi-stage: Vue SPA + Rust Axum binary for Miget

FROM node:22-bookworm AS ui
WORKDIR /ui
COPY ui/package.json ui/package-lock.json ./
RUN npm ci
COPY ui/ ./
RUN npm run build

FROM rust:1-bookworm AS rust
WORKDIR /build
RUN apt-get update && apt-get install -y --no-install-recommends pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo build --release -p loremetry-web \
    && cp target/release/loremetry-web /build/loremetry-web

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=rust /build/loremetry-web /app/loremetry-web
COPY --from=ui /ui/dist /app/ui/dist
ENV PORT=8080
ENV DATA_DIR=/data
ENV STATIC_DIR=/app/ui/dist
ENV RUST_LOG=info
EXPOSE 8080
VOLUME ["/data"]
CMD ["/app/loremetry-web"]
