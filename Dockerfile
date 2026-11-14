# syntax=docker/dockerfile:1

# ---- Build stage ----
FROM rust:1-bookworm AS builder

# RocksDB (cozo-ng-rocks) a besoin d'un toolchain C++
RUN apt-get update && apt-get install -y --no-install-recommends \
    clang libclang-dev cmake pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /src
COPY . .

# RocksDB doit etre active explicitement (storage-rocksdb) en plus de compact (sqlite, requests, graph-algo)
RUN cargo build --release -p cozo-ng-bin --features compact,storage-rocksdb \
    && strip target/release/cozo-bin

# ---- Runtime stage ----
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libsqlite3-0 \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --create-home --home-dir /data cozo

COPY --from=builder /src/target/release/cozo-bin /usr/local/bin/cozo-bin

USER cozo
WORKDIR /data
VOLUME /data
EXPOSE 9070

# 0.0.0.0 est nécessaire pour être joignable depuis l'extérieur du conteneur
ENTRYPOINT ["cozo-bin"]
CMD ["server", "--engine", "rocksdb", "--path", "/data/db", "--bind", "0.0.0.0", "--port", "9070"]