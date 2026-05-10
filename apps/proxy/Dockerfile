# syntax=docker/dockerfile:1.7
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

COPY Cargo.toml Cargo.lock* ./
COPY src ./src

# BuildKit cache mounts persist the cargo registry, git index, and target dir
# across image builds — even when Cargo.toml changes. Cargo's incremental
# compilation reuses what it can.
#
# `target` lives in a cache mount, so it is NOT part of the image layer; we
# copy the built binary out into the workdir before the layer finalizes.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo build --release && \
    cp target/release/proxy /app/proxy

# ---- runtime ----
FROM debian:trixie-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/proxy .

EXPOSE 8080

CMD ["./proxy"]
