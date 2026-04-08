FROM rust:1.91-slim AS builder

# Build dependencies for native crates:
#   cmake, make, gcc, g++    — libz-ng-sys, general C/C++ builds
#   pkg-config, libssl-dev   — openssl-sys
#   protobuf-compiler        — lance-encoding (prost protobuf codegen)
#   libprotobuf-dev          — well-known proto types (google/protobuf/*.proto)
#   perl                     — aws-lc-sys build
RUN apt-get update && apt-get install -y --no-install-recommends \
    cmake make gcc g++ pkg-config libssl-dev \
    protobuf-compiler libprotobuf-dev \
    perl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Stub main so dependencies are cached independently of source changes
RUN mkdir src && echo 'fn main() {}' > src/main.rs
COPY Cargo.toml Cargo.lock* ./
RUN cargo build --release 2>/dev/null || true

# Build the real binary
COPY src ./src
RUN touch src/main.rs && cargo build --release

# ---- runtime ----
FROM debian:trixie-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/proxy .

EXPOSE 8080

CMD ["./proxy"]
