# ベースイメージ
FROM rust:bookworm AS chef
RUN cargo install cargo-chef 

# 解析ステージ
FROM chef AS planner
WORKDIR /usr/src/mastaker
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ビルドステージ
FROM chef AS builder
RUN apt-get update && apt-get install -y clang git \
    && git clone https://github.com/rui314/mold.git \
    && mkdir /mold/build \
    && cd /mold/build \
    && git switch -c v2.4.0 \
    && ../install-build-deps.sh \
    && cmake -DCMAKE_BUILD_TYPE=Release -DCMAKE_CXX_COMPILER=/usr/bin/c++ .. \
    && cmake --build . -j $(nproc) \
    && cmake --install .
WORKDIR /usr/src/mastaker
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
