FROM mcr.microsoft.com/devcontainers/rust:1

# Include lld linker to improve build times either by using environment variable
# RUSTFLAGS="-C link-arg=-fuse-ld=lld" or with Cargo's configuration file (i.e see .cargo/config.toml).
RUN apt-get update && export DEBIAN_FRONTEND=noninteractive \
   && apt-get -y install clang postgresql-client \
   && apt-get autoremove -y && apt-get clean -y \
   && git clone https://github.com/rui314/mold.git \
   && mkdir /mold/build \
   && cd /mold/build \
   && git checkout v1.7.1 \
   && ../install-build-deps.sh \
   && cmake -DCMAKE_BUILD_TYPE=Release -DCMAKE_CXX_COMPILER=/usr/bin/c++ .. \
   && cmake --build . -j $(nproc) \
   && cmake --install .
