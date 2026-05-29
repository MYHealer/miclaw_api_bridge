FROM ubuntu:22.04

ENV DEBIAN_FRONTEND=noninteractive
ENV PNPM_HOME=/root/.local/share/pnpm
ENV PATH=/root/.cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:${PNPM_HOME}

RUN apt-get update \
  && apt-get install -y --no-install-recommends \
    build-essential \
    ca-certificates \
    curl \
    file \
    git \
    libayatana-appindicator3-dev \
    libfuse2 \
    libssl-dev \
    libwebkit2gtk-4.1-dev \
    librsvg2-dev \
    patchelf \
    pkg-config \
    qemu-user \
    rsync \
    wget \
    xz-utils \
  && rm -rf /var/lib/apt/lists/*

ARG WEBKITGTK_VERSION=2.44.3-0ubuntu0.22.04.1
RUN set -eux; \
  mkdir -p /tmp/webkitgtk-pin; \
  cd /tmp/webkitgtk-pin; \
  for pkg in \
    "gir1.2-javascriptcoregtk-4.1_${WEBKITGTK_VERSION}_amd64.deb" \
    "gir1.2-webkit2-4.1_${WEBKITGTK_VERSION}_amd64.deb" \
    "libjavascriptcoregtk-4.1-0_${WEBKITGTK_VERSION}_amd64.deb" \
    "libjavascriptcoregtk-4.1-dev_${WEBKITGTK_VERSION}_amd64.deb" \
    "libwebkit2gtk-4.1-0_${WEBKITGTK_VERSION}_amd64.deb" \
    "libwebkit2gtk-4.1-dev_${WEBKITGTK_VERSION}_amd64.deb"; do \
      curl -fL --retry 5 --retry-delay 2 \
        "https://launchpad.net/ubuntu/+archive/primary/+files/${pkg}" \
        -o "${pkg}"; \
    done; \
  apt-get update; \
  apt-get install -y --allow-downgrades ./*.deb; \
  apt-mark hold \
    libjavascriptcoregtk-4.1-0 \
    libjavascriptcoregtk-4.1-dev \
    gir1.2-javascriptcoregtk-4.1 \
    gir1.2-webkit2-4.1 \
    libwebkit2gtk-4.1-0 \
    libwebkit2gtk-4.1-dev; \
  rm -rf /tmp/webkitgtk-pin /var/lib/apt/lists/*

RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - \
  && apt-get update \
  && apt-get install -y --no-install-recommends nodejs \
  && corepack enable \
  && corepack prepare pnpm@9 --activate \
  && rm -rf /var/lib/apt/lists/*

RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --profile minimal --default-toolchain stable \
  && rustup target add x86_64-unknown-linux-gnu

WORKDIR /work
