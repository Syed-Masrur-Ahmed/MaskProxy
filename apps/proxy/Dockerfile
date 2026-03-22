FROM rust:1.82-slim AS builder

WORKDIR /app

# Stub main so dependencies are cached independently of source changes
RUN mkdir src && echo 'fn main() {}' > src/main.rs
COPY Cargo.toml Cargo.lock* ./
RUN cargo build --release 2>/dev/null || true

# Build the real binary
COPY src ./src
RUN touch src/main.rs && cargo build --release

# ---- runtime ----
FROM debian:bookworm-slim

WORKDIR /app
COPY --from=builder /app/target/release/proxy .

EXPOSE 8080

CMD ["./proxy"]
