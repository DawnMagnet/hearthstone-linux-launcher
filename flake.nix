{
  description = "Native Rust Relm4/libadwaita Hearthstone Linux manager";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      rust-overlay,
    }:
    flake-utils.lib.eachSystem [ "x86_64-linux" ] (
      system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };

        pname = "hearthstone-linux-gui";
        packageVersion = "0.1.5";
        appId = "io.github.hearthstone_linux_gui";
        desktopFile = "${appId}.desktop";
        iconFile = "${appId}.svg";

        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [
            "rust-src"
            "rustfmt"
            "clippy"
          ];
        };

        rustSource = pkgs.lib.cleanSourceWith {
          src = ./.;
          filter =
            path: _type:
            let
              root = toString ./.;
              relative = pkgs.lib.removePrefix "${root}/" (toString path);
              top = builtins.head (pkgs.lib.splitString "/" relative);
            in
            builtins.elem top [
              "Cargo.lock"
              "Cargo.toml"
              "assets"
              "crates"
              "data"
              "src"
            ];
        };

        x11RuntimeInputs = with pkgs; [
          libice
          libsm
          libx11
          libxscrnsaver
          libxcursor
          libxdamage
          libxext
          libxfixes
          libxi
          libxinerama
          libxrandr
          libxrender
          libxtst
          libxxf86vm
          libxcb
        ];

        graphicsRuntimeInputs =
          with pkgs;
          [
            libdrm
            libglvnd
            libxkbcommon
            wayland
            xkeyboard_config
          ]
          ++ x11RuntimeInputs;

        fhsRuntimeInputs =
          with pkgs;
          [
            alsa-lib
            at-spi2-atk
            at-spi2-core
            cairo
            cups
            dbus
            expat
            fontconfig
            freetype
            gcc.cc.lib
            gdk-pixbuf
            glib
            gtk3
            libpulseaudio
            libuuid
            libxml2
            mesa
            nspr
            nss
            openssl
            pango
            vulkan-loader
          ]
          ++ graphicsRuntimeInputs;

        hearthstoneRuntime = pkgs.buildFHSEnv {
          name = "hearthstone-linux-gui-runtime";
          targetPkgs = _: fhsRuntimeInputs;
          runScript = "${pkgs.coreutils}/bin/env";
        };

        nativeBuildInputs = with pkgs; [
          desktop-file-utils
          glib
          gobject-introspection
          gtk4
          libadwaita
          pkg-config
          rustToolchain
          wrapGAppsHook4
        ];

        buildInputs = with pkgs; [
          cairo
          dbus.lib
          gdk-pixbuf
          glib
          graphene
          gtk4
          libadwaita
          libglvnd
          libxcursor
          libxi
          libxinerama
          libxrandr
          libxscrnsaver
          libxxf86vm
          mesa
          pango
        ];

        hearthstonePackage = pkgs.rustPlatform.buildRustPackage {
          inherit pname;
          version = packageVersion;
          src = rustSource;
          cargoLock.lockFile = ./Cargo.lock;
          inherit nativeBuildInputs buildInputs;
          cargoBuildFlags = [ "--workspace" ];
          cargoTestFlags = [ "--workspace" ];

          preFixup = ''
            gappsWrapperArgs+=(
              --set-default HEARTHSTONE_LINUX_RUNNER "${hearthstoneRuntime}/bin/hearthstone-linux-gui-runtime"
            )
          '';

          postInstall = ''
            target_dir="target/${pkgs.stdenv.hostPlatform.config}/release"
            if [ ! -d "$target_dir" ]; then
              target_dir="target/release"
            fi

            install -Dm644 data/${desktopFile} \
              $out/share/applications/${desktopFile}
            install -Dm644 data/${appId}.metainfo.xml \
              $out/share/metainfo/${appId}.metainfo.xml
            install -Dm644 ${./packaging/appimage/io.github.hearthstone_linux_gui.svg} \
              $out/share/icons/hicolor/scalable/apps/${iconFile}
            install -Dm644 assets/client.config.in \
              $out/share/hearthstone-linux-gui/client.config.in
            install -Dm755 "$target_dir/libCoreFoundation.so" \
              $out/share/hearthstone-linux-gui/stubs/CoreFoundation.so
            install -Dm755 "$target_dir/libOSXWindowManagement.so" \
              $out/share/hearthstone-linux-gui/stubs/libOSXWindowManagement.so
            install -Dm755 "$target_dir/libblz_commerce_sdk_plugin.so" \
              $out/share/hearthstone-linux-gui/stubs/libblz_commerce_sdk_plugin.so
            install -Dm755 "$target_dir/libcommerce_http_client.so" \
              $out/share/hearthstone-linux-gui/stubs/libcommerce_http_client.so
            install -Dm755 "$target_dir/libNativeApiMac.so" \
              $out/share/hearthstone-linux-gui/stubs/libNativeApiMac.so
          '';
        };

        appimage = import ./nix/appimage.nix {
          inherit
            pkgs
            pname
            packageVersion
            appId
            desktopFile
            iconFile
            hearthstonePackage
            ;
        };
      in
      {
        packages.default = hearthstonePackage;
        packages.runtime = hearthstoneRuntime;
        packages.AppDir = appimage.appDir;
        packages.AppImage = appimage.appImage;

        devShells.default = pkgs.mkShell {
          inherit nativeBuildInputs buildInputs;
          packages = with pkgs; [
            appimage-run
            hearthstoneRuntime
            rust-analyzer
          ];
          HEARTHSTONE_LINUX_RUNNER = "${hearthstoneRuntime}/bin/hearthstone-linux-gui-runtime";
          RUST_BACKTRACE = "1";
        };

        apps.default = flake-utils.lib.mkApp {
          drv = self.packages.${system}.default;
        };
      }
    );
}
