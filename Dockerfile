# ビルドステージ
FROM rust:bookworm as builder
WORKDIR /usr/src/mastaker
COPY . .
RUN cargo install --path .

# 実行ステージ
FROM debian:bookworm-slim
WORKDIR /usr/local/bin
# ビルドステージからビルドされたバイナリをコピー
COPY --from=builder /usr/local/cargo/bin/mastaker .
# SSLの証明書 (reqwestや他のHTTPライブラリでHTTPSを使う場合に必要)
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
CMD ["mastaker"]
