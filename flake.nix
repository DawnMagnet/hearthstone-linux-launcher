{
  description = "Native Rust GTK4/libadwaita Hearthstone Linux manager";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rustfmt" "clippy" ];
        };
        source = pkgs.lib.cleanSourceWith {
          src = ./.;
          filter = path: type:
            let
              root = toString ./.;
              relative = pkgs.lib.removePrefix "${root}/" (toString path);
              top = builtins.head (pkgs.lib.splitString "/" relative);
            in !(builtins.elem top [
              ".direnv"
              "AppDir"
              "result"
              "target"
            ]);
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
      in {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "hearthstone-linux";
          version = "0.1.0";
          src = source;
          cargoLock.lockFile = ./Cargo.lock;
          inherit nativeBuildInputs buildInputs;
          cargoBuildFlags = [ "--workspace" ];
          cargoTestFlags = [ "--workspace" ];

          postInstall = ''
            target_dir="target/${pkgs.stdenv.hostPlatform.config}/release"
            if [ ! -d "$target_dir" ]; then
              target_dir="target/release"
            fi

            install -Dm644 data/io.github.hearthstone_linux.desktop \
              $out/share/applications/io.github.hearthstone_linux.desktop
            install -Dm644 data/io.github.hearthstone_linux.metainfo.xml \
              $out/share/metainfo/io.github.hearthstone_linux.metainfo.xml
            install -Dm644 assets/client.config.in \
              $out/share/hearthstone-linux/client.config.in
            install -Dm755 "$target_dir/libCoreFoundation.so" \
              $out/share/hearthstone-linux/stubs/CoreFoundation.so
            install -Dm755 "$target_dir/libOSXWindowManagement.so" \
              $out/share/hearthstone-linux/stubs/libOSXWindowManagement.so
            install -Dm755 "$target_dir/libblz_commerce_sdk_plugin.so" \
              $out/share/hearthstone-linux/stubs/libblz_commerce_sdk_plugin.so
          '';
        };

        devShells.default = pkgs.mkShell {
          inherit nativeBuildInputs buildInputs;
          packages = with pkgs; [
            appimage-run
            rust-analyzer
          ];
          RUST_BACKTRACE = "1";
        };

        apps.default = flake-utils.lib.mkApp {
          drv = self.packages.${system}.default;
        };
      });
}
