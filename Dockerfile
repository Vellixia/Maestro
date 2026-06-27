# ── Stage 1: cargo-chef install ───────────────────────────────────────────────
FROM lukemathwalker/cargo-chef:latest-rust-1 AS chef
WORKDIR /app

# ── Stage 2: compute recipe (dependency fingerprint) ─────────────────────────
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ── Stage 3: cook deps (cached), then build release binary ───────────────────
FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
# Cook only deps — this layer is cached as long as Cargo.lock doesn't change.
RUN cargo chef cook --release --recipe-path recipe.json

COPY . .
RUN cargo build --release --bin maestro

# ── Stage 4: lean runtime image ───────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/maestro /usr/local/bin/maestro

ENV RUST_LOG=info
EXPOSE 3456

CMD ["maestro"]
