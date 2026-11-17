# rabbiz-de arm64 build container (M0-T8).
#   docker build -f tools/rabbiz-de/Dockerfile -t rabbiz-de-build .
#   docker run --rm -v "$PWD:/src" -w /src/tools/rabbiz-de rabbiz-de-build \
#       cargo build --release -p de-bin
FROM docker.io/library/rust:1.99-bookworm
RUN dpkg --add-architecture arm64 && apt-get update && apt-get install -y --no-install-recommends \
      gcc-aarch64-linux-gnu pkg-config \
      libdrm-dev:arm64 libgbm-dev:arm64 libegl1-mesa-dev:arm64 \
      libinput-dev:arm64 libseat-dev:arm64 libudev-dev:arm64 \
      libpixman-1-dev:arm64 libxkbcommon-dev:arm64 libwayland-dev:arm64 \
      wayland-protocols:arm64 libx11-dev:arm64 libxcursor-dev:arm64 \
      libxcb1-dev:arm64 libxi-dev:arm64 && rm -rf /var/lib/apt/lists/*
RUN rustup target add aarch64-unknown-linux-gnu
ENV CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
    PKG_CONFIG_ALLOW_CROSS=1 \
    PKG_CONFIG_PATH=/usr/lib/aarch64-linux-gnu/pkgconfig
