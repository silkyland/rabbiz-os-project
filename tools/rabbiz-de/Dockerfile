FROM rust:1.99-bookworm

RUN apt-get update -qq && DEBIAN_FRONTEND=noninteractive apt-get install -y -qq --no-install-recommends \
    pkg-config \
    clang \
    libclang-dev \
    libseat-dev \
    libgbm-dev \
    libinput-dev \
    libudev-dev \
    libegl-dev \
    libgles2-mesa-dev \
    libxkbcommon-dev \
    libpixman-1-dev \
    libdrm-dev \
    libwayland-dev \
    && rm -rf /var/lib/apt/lists/*

ENV PATH="/usr/local/cargo/bin:${PATH}"
