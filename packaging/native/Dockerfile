FROM debian:12 AS build

ARG RUST_VERSION=1.92.0
ARG NFPM_VERSION=2.43.1
ARG TARGETARCH=amd64

ENV CARGO_HOME=/opt/cargo \
  RUSTUP_HOME=/opt/rustup \
  PATH=/opt/cargo/bin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
  DEBIAN_FRONTEND=noninteractive

WORKDIR /workspace

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

RUN apt-get update \
  && apt-get install -y --no-install-recommends \
  ca-certificates \
  curl \
  file \
  gcc \
  libc6-dev \
  libadwaita-1-dev \
  libdbus-1-dev \
  libgdk-pixbuf-2.0-dev \
  libgtk-4-dev \
  librsvg2-bin \
  make \
  pkg-config \
  xz-utils \
  && rm -rf /var/lib/apt/lists/*

RUN case "$TARGETARCH" in \
  amd64) rustup_arch=x86_64-unknown-linux-gnu; nfpm_arch=x86_64 ;; \
  arm64) rustup_arch=aarch64-unknown-linux-gnu; nfpm_arch=arm64 ;; \
  *) echo "unsupported TARGETARCH=$TARGETARCH" >&2; exit 1 ;; \
  esac \
  && curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain "$RUST_VERSION" --default-host "$rustup_arch" \
  && curl -fsSL "https://github.com/goreleaser/nfpm/releases/download/v${NFPM_VERSION}/nfpm_${NFPM_VERSION}_Linux_${nfpm_arch}.tar.gz" \
  | tar -xz -C /usr/local/bin nfpm \
  && chmod 755 /usr/local/bin/nfpm

COPY Cargo.toml Cargo.lock ./
COPY assets ./assets
COPY crates ./crates
COPY data ./data
COPY packaging ./packaging
COPY src ./src

RUN set -eux; \
  case "$TARGETARCH" in \
    amd64) package_arch=amd64; rpm_arch=x86_64; pacman_arch=x86_64 ;; \
    arm64) package_arch=arm64; rpm_arch=aarch64; pacman_arch=aarch64 ;; \
    *) echo "unsupported TARGETARCH=$TARGETARCH" >&2; exit 1 ;; \
  esac; \
  cargo build --workspace --release; \
  cargo test --workspace --release; \
  install -Dm755 target/release/hearthstone-linux-gui /pkgroot/usr/bin/hearthstone-linux-gui; \
  install -Dm755 target/release/libCoreFoundation.so /pkgroot/usr/share/hearthstone-linux-gui/stubs/CoreFoundation.so; \
  install -Dm755 target/release/libOSXWindowManagement.so /pkgroot/usr/share/hearthstone-linux-gui/stubs/libOSXWindowManagement.so; \
  install -Dm755 target/release/libblz_commerce_sdk_plugin.so /pkgroot/usr/share/hearthstone-linux-gui/stubs/libblz_commerce_sdk_plugin.so; \
  install -Dm755 target/release/libcommerce_http_client.so /pkgroot/usr/share/hearthstone-linux-gui/stubs/libcommerce_http_client.so; \
  install -Dm755 target/release/libNativeApiMac.so /pkgroot/usr/share/hearthstone-linux-gui/stubs/libNativeApiMac.so; \
  install -Dm644 assets/client.config.in /pkgroot/usr/share/hearthstone-linux-gui/client.config.in; \
  install -Dm644 data/io.github.hearthstone_linux_gui.desktop /pkgroot/usr/share/applications/io.github.hearthstone_linux_gui.desktop; \
  install -Dm644 data/io.github.hearthstone_linux_gui.metainfo.xml /pkgroot/usr/share/metainfo/io.github.hearthstone_linux_gui.metainfo.xml; \
  install -Dm644 packaging/appimage/io.github.hearthstone_linux_gui.svg /pkgroot/usr/share/icons/hicolor/scalable/apps/io.github.hearthstone_linux_gui.svg; \
  install -d /pkgroot/usr/share/icons/hicolor/128x128/apps /pkgroot/usr/share/icons/hicolor/256x256/apps; \
  rsvg-convert -w 128 -h 128 packaging/appimage/io.github.hearthstone_linux_gui.svg -o /pkgroot/usr/share/icons/hicolor/128x128/apps/io.github.hearthstone_linux_gui.png; \
  rsvg-convert -w 256 -h 256 packaging/appimage/io.github.hearthstone_linux_gui.svg -o /pkgroot/usr/share/icons/hicolor/256x256/apps/io.github.hearthstone_linux_gui.png; \
  version="$(awk -F '"' '/^version = / { print $2; exit }' Cargo.toml)"; \
  mkdir -p /dist /tmp/nfpm; \
  printf '%s\n' \
  'name: hearthstone-linux-gui' \
  "arch: ${package_arch}" \
  'platform: linux' \
  "version: ${version}" \
  'section: games' \
  'priority: optional' \
  'maintainer: hearthstone-linux-gui contributors' \
  'homepage: https://github.com/DawnMagnet/hearthstone-linux' \
  'license: MIT' \
  'description: Native GTK4 Linux GUI manager for installing, logging into, and launching Hearthstone.' \
  'contents:' \
  '  - src: /pkgroot/usr/bin/hearthstone-linux-gui' \
  '    dst: /usr/bin/hearthstone-linux-gui' \
  '  - src: /pkgroot/usr/share/hearthstone-linux-gui' \
  '    dst: /usr/share/hearthstone-linux-gui' \
  '  - src: /pkgroot/usr/share/applications/io.github.hearthstone_linux_gui.desktop' \
  '    dst: /usr/share/applications/io.github.hearthstone_linux_gui.desktop' \
  '  - src: /pkgroot/usr/share/metainfo/io.github.hearthstone_linux_gui.metainfo.xml' \
  '    dst: /usr/share/metainfo/io.github.hearthstone_linux_gui.metainfo.xml' \
  '  - src: /pkgroot/usr/share/icons/hicolor/scalable/apps/io.github.hearthstone_linux_gui.svg' \
  '    dst: /usr/share/icons/hicolor/scalable/apps/io.github.hearthstone_linux_gui.svg' \
  '  - src: /pkgroot/usr/share/icons/hicolor/128x128/apps/io.github.hearthstone_linux_gui.png' \
  '    dst: /usr/share/icons/hicolor/128x128/apps/io.github.hearthstone_linux_gui.png' \
  '  - src: /pkgroot/usr/share/icons/hicolor/256x256/apps/io.github.hearthstone_linux_gui.png' \
  '    dst: /usr/share/icons/hicolor/256x256/apps/io.github.hearthstone_linux_gui.png' \
  'overrides:' \
  '  deb:' \
  '    depends:' \
  '      - libc6 (>= 2.36)' \
  '      - libadwaita-1-0 (>= 1.2)' \
  '      - libgtk-4-1 (>= 4.8)' \
  '      - libglib2.0-0 (>= 2.74)' \
  '      - libdbus-1-3' \
  '      - xdg-utils' \
  '  rpm:' \
  '    depends:' \
  '      - glibc' \
  '      - libadwaita' \
  '      - gtk4' \
  '      - glib2' \
  '      - dbus-libs' \
  '      - xdg-utils' \
  '  archlinux:' \
  '    depends:' \
  '      - glibc' \
  '      - libadwaita' \
  '      - gtk4' \
  '      - glib2' \
  '      - dbus' \
  '      - xdg-utils' \
  > /tmp/nfpm/nfpm.yaml; \
  nfpm package --config /tmp/nfpm/nfpm.yaml --packager deb --target "/dist/hearthstone-linux-gui_${version}_${package_arch}.deb"; \
  nfpm package --config /tmp/nfpm/nfpm.yaml --packager rpm --target "/dist/hearthstone-linux-gui-${version}-1.${rpm_arch}.rpm"; \
  nfpm package --config /tmp/nfpm/nfpm.yaml --packager archlinux --target "/dist/hearthstone-linux-gui-${version}-1-${pacman_arch}.pkg.tar.zst"

FROM scratch AS dist
COPY --from=build /dist /
