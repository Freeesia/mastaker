# ベースイメージ
FROM rust:bookworm AS chef
RUN cargo install cargo-chef 
WORKDIR /usr/src/mastaker

# 解析ステージ
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ビルドステージ
FROM chef AS builder
COPY --from=planner /usr/src/mastaker/recipe.json recipe.json
# Build dependencies - this is the caching Docker layer!
RUN cargo chef cook --release --recipe-path recipe.json
# Build application
COPY . .
RUN cargo build --release

# 実行ステージ
FROM debian:bookworm-slim
WORKDIR /usr/local/bin
# SSLの証明書 (reqwestや他のHTTPライブラリでHTTPSを使う場合に必要)
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
# ビルドステージからビルドされたバイナリをコピー
COPY --from=builder /usr/src/mastaker/target/release/mastaker .
CMD ["mastaker"]
