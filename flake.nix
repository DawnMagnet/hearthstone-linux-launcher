{
  description = "Native Rust GTK4/libadwaita hearthstone-linux-gui manager";

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
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [
            "rust-src"
            "rustfmt"
            "clippy"
          ];
        };
        source = pkgs.lib.cleanSourceWith {
          src = ./.;
          filter =
            path: type:
            let
              root = toString ./.;
              relative = pkgs.lib.removePrefix "${root}/" (toString path);
              top = builtins.head (pkgs.lib.splitString "/" relative);
            in
            !(builtins.elem top [
              ".direnv"
              "AppDir"
              "result"
              "target"
            ]);
        };
        pname = "hearthstone-linux-gui";
        packageVersion = "0.1.1";
        appId = "io.github.hearthstone_linux_gui";
        appImageFile = "hearthstone-linux-gui-x86_64.AppImage";
        desktopFile = "${appId}.desktop";
        iconFile = "${appId}.svg";
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
        portableRuntimeInputs =
          with pkgs;
          [
            alsa-lib
            at-spi2-atk
            at-spi2-core
            cairo
            dbus.lib
            dconf.lib
            expat
            fontconfig
            freetype
            gdk-pixbuf
            glib
            glib-networking
            glibc
            graphene
            gsettings-desktop-schemas
            gtk4
            libadwaita
            libepoxy
            libpulseaudio
            pango
            zlib
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
          src = source;
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
            install -Dm644 assets/client.config.in \
              $out/share/hearthstone-linux-gui/client.config.in
            install -Dm755 "$target_dir/libCoreFoundation.so" \
              $out/share/hearthstone-linux-gui/stubs/CoreFoundation.so
            install -Dm755 "$target_dir/libOSXWindowManagement.so" \
              $out/share/hearthstone-linux-gui/stubs/libOSXWindowManagement.so
            install -Dm755 "$target_dir/libblz_commerce_sdk_plugin.so" \
              $out/share/hearthstone-linux-gui/stubs/libblz_commerce_sdk_plugin.so
          '';
        };
        appimageTool = pkgs.appimageTools.extractType2 {
          pname = "appimagetool";
          version = "12";
          src = pkgs.fetchurl {
            url = "https://github.com/AppImage/AppImageKit/releases/download/12/appimagetool-x86_64.AppImage";
            sha256 = "04ws94q71bwskmhizhwmaf41ma4wabvfgjgkagr8wf3vakgv866r";
          };
        };
        portableLibraryPath = pkgs.lib.makeLibraryPath portableRuntimeInputs;
        appDir =
          pkgs.runCommand "hearthstone-linux-gui-AppDir"
            {
              nativeBuildInputs = with pkgs; [
                binutils
                coreutils
                desktop-file-utils
                findutils
                glib
                patchelf
              ];
            }
            ''
              mkdir -p \
                $out/usr/bin \
                $out/usr/lib \
                $out/usr/lib/hearthstone-linux-gui-runtime \
                $out/usr/share/applications \
                $out/usr/share/icons/hicolor/scalable/apps \
                $out/usr/share/metainfo

              install -Dm755 ${hearthstonePackage}/bin/.hearthstone-linux-gui-wrapped \
                $out/usr/bin/hearthstone-linux-gui
              cp -a ${hearthstonePackage}/share/. $out/usr/share/
              install -Dm644 ${./packaging/appimage/io.github.hearthstone_linux_gui.svg} \
                $out/usr/share/icons/hicolor/scalable/apps/${iconFile}
              install -Dm755 ${./packaging/appimage/AppRun} $out/AppRun

              ln -s usr/share/applications/${desktopFile} $out/${desktopFile}
              ln -s usr/share/icons/hicolor/scalable/apps/${iconFile} $out/${iconFile}

              copy_lib() {
                local lib="$1"
                local name
                name="$(basename "$lib")"
                if [ -L "$lib" ]; then
                  local target target_name
                  target="$(readlink -f "$lib")"
                  target_name="$(basename "$target")"
                  if [ "$name" = "$target_name" ]; then
                    install -Dm755 "$target" "$out/usr/lib/$name"
                  elif [ ! -e "$out/usr/lib/$target_name" ]; then
                    install -Dm755 "$target" "$out/usr/lib/$target_name"
                    ln -sfn "$target_name" "$out/usr/lib/$name"
                  else
                    ln -sfn "$target_name" "$out/usr/lib/$name"
                  fi
                else
                  install -Dm755 "$lib" "$out/usr/lib/$name"
                fi
              }

              IFS=: read -r -a lib_dirs <<< "${portableLibraryPath}"
              for dir in "''${lib_dirs[@]}"; do
                if [ -d "$dir" ]; then
                  while IFS= read -r lib; do
                    copy_lib "$lib"
                  done < <(find "$dir" -maxdepth 1 \( -type f -o -type l \) -name '*.so*')
                fi
              done
              for lib in \
                ${pkgs.stdenv.cc.cc.lib}/lib/libstdc++.so* \
                ${pkgs.stdenv.cc.cc.lib}/lib/libgcc_s.so*; do
                [ -e "$lib" ] || continue
                copy_lib "$lib"
              done

              for input in ${pkgs.lib.concatStringsSep " " portableRuntimeInputs}; do
                if [ -d "$input/lib/gio/modules" ]; then
                  mkdir -p $out/usr/lib/gio/modules
                  for module in "$input"/lib/gio/modules/*.so; do
                    [ -e "$module" ] || continue
                    install -Dm755 "$module" "$out/usr/lib/gio/modules/$(basename "$module")"
                  done
                fi
                if [ -d "$input/lib/gdk-pixbuf-2.0" ]; then
                  mkdir -p $out/usr/lib/gdk-pixbuf-2.0
                  cp -aL "$input"/lib/gdk-pixbuf-2.0/. $out/usr/lib/gdk-pixbuf-2.0/
                  chmod -R u+w $out/usr/lib/gdk-pixbuf-2.0
                fi
                if [ -d "$input/share/glib-2.0/schemas" ]; then
                  mkdir -p $out/usr/share/glib-2.0/schemas
                  cp -L "$input"/share/glib-2.0/schemas/*.xml $out/usr/share/glib-2.0/schemas/ 2>/dev/null || true
                fi
                if [ -d "$input/share/gsettings-schemas" ]; then
                  find "$input/share/gsettings-schemas" -path '*/glib-2.0/schemas/*.xml' \
                    -exec cp -L {} $out/usr/share/glib-2.0/schemas/ \;
                fi
              done

              if [ -d $out/usr/share/glib-2.0/schemas ]; then
                glib-compile-schemas $out/usr/share/glib-2.0/schemas
              fi

              install -Dm755 ${pkgs.glibc.out}/lib/ld-linux-x86-64.so.2 \
                $out/usr/lib/hearthstone-linux-gui-runtime/ld-linux-x86-64.so.2
              install -Dm755 ${pkgs.patchelf}/bin/patchelf \
                $out/usr/lib/hearthstone-linux-gui-runtime/patchelf
              cat > $out/usr/bin/patchelf <<'EOF'
              #!/usr/bin/env sh
              appdir="$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)"
              lib_path="$appdir/usr/lib:$appdir/usr/lib/hearthstone-linux-gui-runtime''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
              exec "$appdir/usr/lib/hearthstone-linux-gui-runtime/ld-linux-x86-64.so.2" \
                --library-path "$lib_path" \
                "$appdir/usr/lib/hearthstone-linux-gui-runtime/patchelf" "$@"
              EOF
              chmod 755 $out/usr/bin/patchelf

              patchelf --set-rpath '$ORIGIN/../lib:$ORIGIN/../lib/hearthstone-linux-gui-runtime' \
                $out/usr/bin/hearthstone-linux-gui
              patchelf --set-rpath '$ORIGIN:$ORIGIN/../../usr/lib:$ORIGIN/../../usr/lib/hearthstone-linux-gui-runtime' \
                $out/usr/lib/hearthstone-linux-gui-runtime/patchelf
              find $out/usr/bin $out/usr/lib -type f -executable \
                -exec strip --strip-unneeded {} + 2>/dev/null || true
            '';
        appImage =
          pkgs.runCommand appImageFile
            {
              nativeBuildInputs = with pkgs; [
                coreutils
                desktop-file-utils
                patchelf
                squashfsTools
              ];
            }
            ''
              mkdir -p $out
              cp -a ${appimageTool} appimagetool
              chmod -R u+w appimagetool
              patchelf --set-interpreter ${pkgs.glibc.out}/lib/ld-linux-x86-64.so.2 \
                --set-rpath ${
                  pkgs.lib.makeLibraryPath [
                    pkgs.glibc
                    pkgs.zlib
                    pkgs.glib
                    pkgs.libuuid
                  ]
                }:$(pwd)/appimagetool/usr/lib \
                appimagetool/usr/bin/appimagetool
              patchelf --set-interpreter ${pkgs.glibc.out}/lib/ld-linux-x86-64.so.2 \
                --set-rpath ${
                  pkgs.lib.makeLibraryPath [
                    pkgs.glibc
                    pkgs.zlib
                  ]
                } \
                appimagetool/usr/lib/appimagekit/mksquashfs
              cp -a ${appDir} AppDir
              chmod -R u+w AppDir
              ARCH=x86_64 ./appimagetool/usr/bin/appimagetool \
                AppDir $out/${appImageFile}
            '';
        packageFromAppImage =
          packager: extension:
          pkgs.runCommand "hearthstone-linux-gui-${extension}"
            {
              nativeBuildInputs = with pkgs; [
                coreutils
                nfpm
              ];
            }
            ''
              mkdir -p root/opt/hearthstone-linux-gui root/usr/bin \
                root/usr/share/applications root/usr/share/icons/hicolor/scalable/apps $out

              install -Dm755 ${appImage}/${appImageFile} \
                root/opt/hearthstone-linux-gui/${appImageFile}
              install -Dm644 ${./data/io.github.hearthstone_linux_gui.desktop} \
                root/usr/share/applications/${desktopFile}
              install -Dm644 ${./packaging/appimage/io.github.hearthstone_linux_gui.svg} \
                root/usr/share/icons/hicolor/scalable/apps/${iconFile}

              cat > root/usr/bin/hearthstone-linux-gui <<'EOF'
              #!/usr/bin/env sh
              export APPIMAGE_EXTRACT_AND_RUN="''${APPIMAGE_EXTRACT_AND_RUN:-1}"
              exec /opt/hearthstone-linux-gui/${appImageFile} "$@"
              EOF
              chmod 755 root/usr/bin/hearthstone-linux-gui

              cat > nfpm.yaml <<EOF
              name: hearthstone-linux-gui
              arch: amd64
              platform: linux
              version: "${packageVersion}"
              section: games
              priority: optional
              maintainer: hearthstone-linux-gui contributors
              description: Native GTK4 Linux GUI manager for installing, logging into, and launching Hearthstone.
              license: MIT
              contents:
                - src: $(pwd)/root/opt/hearthstone-linux-gui/${appImageFile}
                  dst: /opt/hearthstone-linux-gui/${appImageFile}
                  file_info:
                    mode: 0755
                - src: $(pwd)/root/usr/bin/hearthstone-linux-gui
                  dst: /usr/bin/hearthstone-linux-gui
                  file_info:
                    mode: 0755
                - src: $(pwd)/root/usr/share/applications/${desktopFile}
                  dst: /usr/share/applications/${desktopFile}
                - src: $(pwd)/root/usr/share/icons/hicolor/scalable/apps/${iconFile}
                  dst: /usr/share/icons/hicolor/scalable/apps/${iconFile}
              EOF

              nfpm package --config nfpm.yaml --packager ${packager} --target $out
            '';
        debPackage = packageFromAppImage "deb" "deb";
        rpmPackage = packageFromAppImage "rpm" "rpm";
      in
      {
        packages.default = hearthstonePackage;

        packages.runtime = hearthstoneRuntime;
        packages.AppDir = appDir;
        packages.AppImage = appImage;
        packages.Deb = debPackage;
        packages.Rpm = rpmPackage;
        packages.AllDist = pkgs.runCommand "hearthstone-linux-gui-AllDist" { } ''
          mkdir -p $out/nix $out/appimage $out/deb $out/rpm
          ln -s ${hearthstonePackage} $out/nix/hearthstone-linux-gui
          ln -s ${hearthstoneRuntime} $out/nix/hearthstone-linux-gui-runtime
          cp ${appImage}/${appImageFile} $out/appimage/
          cp ${debPackage}/*.deb $out/deb/
          cp ${rpmPackage}/*.rpm $out/rpm/
          cat > $out/README.txt <<EOF
          hearthstone-linux-gui distribution artifacts

          nix/hearthstone-linux-gui          Native Nix package output
          nix/hearthstone-linux-gui-runtime  Nix runtime wrapper for the Unity player
          appimage/*.AppImage                Portable x86_64 AppImage
          deb/*.deb                          Debian package that installs the AppImage payload
          rpm/*.rpm                          RPM package that installs the AppImage payload
          EOF
        '';

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
