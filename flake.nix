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
        rustSource = pkgs.lib.cleanSourceWith {
          src = ./.;
          filter =
            path: type:
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
        pname = "hearthstone-linux-gui";
        packageVersion = "0.1.5";
        appId = "io.github.hearthstone_linux_gui";
        appImageFile = "${pname}-${packageVersion}-x86_64.AppImage";
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
        appimageTool = pkgs.appimageTools.extractType2 {
          pname = "appimagetool";
          version = "continuous";
          src = pkgs.fetchurl {
            url = "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage";
            sha256 = "sha256-ptceK2zWb46NFsN60WRliYXgz1/KqVDJCkgokMudE+A=";
          };
        };
        appimageRuntime = pkgs.fetchurl {
          url = "https://github.com/AppImage/type2-runtime/releases/download/continuous/runtime-x86_64";
          sha256 = "sha256-okGdzkdWg5WuecAf+ppaNB3TOVgTUv8QTQc1J1Qxd+U=";
        };
        appDir =
          pkgs.runCommand "hearthstone-linux-gui-AppDir"
            {
              nativeBuildInputs = with pkgs; [
                binutils
                coreutils
                desktop-file-utils
                findutils
                glib
                librsvg
                patchelf
                pax-utils
              ];
            }
            ''
              mkdir -p \
                $out/usr/bin \
                $out/usr/lib \
                $out/usr/lib/hearthstone-linux-gui-runtime \
                $out/usr/share/applications \
                $out/usr/share/icons/hicolor/128x128/apps \
                $out/usr/share/icons/hicolor/256x256/apps \
                $out/usr/share/icons/hicolor/scalable/apps \
                $out/usr/share/metainfo

              install -Dm755 ${hearthstonePackage}/bin/.hearthstone-linux-gui-wrapped \
                $out/usr/bin/hearthstone-linux-gui
              cp -a ${hearthstonePackage}/share/. $out/usr/share/
              chmod -R u+w $out/usr/share
              rsvg-convert -w 128 -h 128 ${./packaging/appimage/io.github.hearthstone_linux_gui.svg} \
                -o $out/usr/share/icons/hicolor/128x128/apps/${appId}.png
              rsvg-convert -w 256 -h 256 ${./packaging/appimage/io.github.hearthstone_linux_gui.svg} \
                -o $out/usr/share/icons/hicolor/256x256/apps/${appId}.png
              install -Dm755 ${./packaging/appimage/AppRun} $out/AppRun

              ln -s usr/share/applications/${desktopFile} $out/${desktopFile}
              ln -s usr/share/icons/hicolor/scalable/apps/${iconFile} $out/${iconFile}
              ln -s usr/share/icons/hicolor/256x256/apps/${appId}.png $out/${appId}.png
              ln -s ${appId}.png $out/.DirIcon

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

              copy_elf_deps() {
                local elf="$1"
                readelf -d "$elf" >/dev/null 2>&1 || return 0
                while IFS= read -r dep; do
                  case "$dep" in
                    /nix/store/*/lib/*.so*)
                      copy_lib "$dep"
                      ;;
                  esac
                done < <(lddtree -l "$elf")
              }

              copy_elf_deps $out/usr/bin/hearthstone-linux-gui

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
                if [ -d "$input/share/X11/xkb" ]; then
                  mkdir -p $out/usr/share/X11
                  cp -aL "$input/share/X11/xkb" $out/usr/share/X11/
                  chmod -R u+w $out/usr/share/X11/xkb
                fi
              done

              for module_dir in $out/usr/lib/gio/modules $out/usr/lib/gdk-pixbuf-2.0; do
                [ -d "$module_dir" ] || continue
                while IFS= read -r -d "" module; do
                  copy_elf_deps "$module"
                done < <(find "$module_dir" -type f -name '*.so' -print0)
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
              copy_elf_deps $out/usr/lib/hearthstone-linux-gui-runtime/patchelf

              fix_portable_elf() {
                local elf="$1"
                case "$(basename "$elf")" in
                  ld-*.so* | ld-linux*.so*)
                    return 0
                    ;;
                esac
                readelf -d "$elf" >/dev/null 2>&1 || return 0

                while IFS= read -r needed; do
                  local basename_needed
                  basename_needed="$(basename "$needed")"
                  if [ -e "$out/usr/lib/$basename_needed" ]; then
                    patchelf --replace-needed "$needed" "$basename_needed" "$elf"
                  fi
                done < <(
                  readelf -d "$elf" \
                    | sed -n 's/.*Shared library: \[\(\/nix\/store\/[^]]*\/lib\/[^]]*\)\].*/\1/p'
                )
              }

              while IFS= read -r -d "" elf; do
                fix_portable_elf "$elf"
              done < <(find $out/usr/bin $out/usr/lib -type f -print0)

              patchelf --set-rpath '$ORIGIN/../lib:$ORIGIN/../lib/hearthstone-linux-gui-runtime' \
                $out/usr/bin/hearthstone-linux-gui
              patchelf --set-rpath '$ORIGIN:$ORIGIN/../../usr/lib:$ORIGIN/../../usr/lib/hearthstone-linux-gui-runtime' \
                $out/usr/lib/hearthstone-linux-gui-runtime/patchelf
              while IFS= read -r -d "" elf; do
                readelf -d "$elf" >/dev/null 2>&1 || continue
                case "$elf" in
                  $out/usr/bin/* | $out/usr/lib/hearthstone-linux-gui-runtime/patchelf)
                    ;;
                  */ld-*.so* | */ld-linux*.so*)
                    ;;
                  *)
                    patchelf --set-rpath '$ORIGIN:$ORIGIN/hearthstone-linux-gui-runtime' "$elf"
                    ;;
                esac
              done < <(find $out/usr/lib -type f -print0)

              find $out/usr/bin $out/usr/lib -type f -executable \
                ! -name 'ld-*.so*' \
                ! -name 'ld-linux*.so*' \
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
              patch_dynamic_tool() {
                local tool="$1"
                local rpath="$2"
                [ -e "$tool" ] || return 0
                readelf -l "$tool" 2>/dev/null | grep -q 'Requesting program interpreter' || return 0
                patchelf --set-interpreter ${pkgs.glibc.out}/lib/ld-linux-x86-64.so.2 \
                  --set-rpath "$rpath" \
                  "$tool"
              }

              patch_dynamic_tool appimagetool/usr/bin/appimagetool \
                "${
                  pkgs.lib.makeLibraryPath [
                    pkgs.glibc
                    pkgs.zlib
                    pkgs.glib
                    pkgs.libuuid
                  ]
                }:$(pwd)/appimagetool/usr/lib"
              patch_dynamic_tool appimagetool/usr/lib/appimagekit/mksquashfs \
                "${
                  pkgs.lib.makeLibraryPath [
                    pkgs.glibc
                    pkgs.zlib
                  ]
                }"
              cp -a ${appDir} AppDir
              chmod -R u+w AppDir
              ARCH=x86_64 ./appimagetool/usr/bin/appimagetool \
                --runtime-file ${appimageRuntime} \
                AppDir $out/${appImageFile}
            '';
        packageFromAppImage =
          packager: extension:
          pkgs.runCommand "${pname}-${packageVersion}-${extension}"
            {
              nativeBuildInputs = with pkgs; [
                coreutils
                librsvg
                nfpm
              ];
            }
            ''
              mkdir -p root/opt/hearthstone-linux-gui root/usr/bin \
                root/usr/share/applications \
                root/usr/share/icons/hicolor/128x128/apps \
                root/usr/share/icons/hicolor/256x256/apps \
                root/usr/share/icons/hicolor/scalable/apps \
                $out

              install -Dm755 ${appImage}/${appImageFile} \
                root/opt/hearthstone-linux-gui/${appImageFile}
              install -Dm644 ${./data/io.github.hearthstone_linux_gui.desktop} \
                root/usr/share/applications/${desktopFile}
              install -Dm644 ${./packaging/appimage/io.github.hearthstone_linux_gui.svg} \
                root/usr/share/icons/hicolor/scalable/apps/${iconFile}
              rsvg-convert -w 128 -h 128 ${./packaging/appimage/io.github.hearthstone_linux_gui.svg} \
                -o root/usr/share/icons/hicolor/128x128/apps/${appId}.png
              rsvg-convert -w 256 -h 256 ${./packaging/appimage/io.github.hearthstone_linux_gui.svg} \
                -o root/usr/share/icons/hicolor/256x256/apps/${appId}.png

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
                - src: $(pwd)/root/usr/share/icons/hicolor/128x128/apps/${appId}.png
                  dst: /usr/share/icons/hicolor/128x128/apps/${appId}.png
                - src: $(pwd)/root/usr/share/icons/hicolor/256x256/apps/${appId}.png
                  dst: /usr/share/icons/hicolor/256x256/apps/${appId}.png
              EOF

              nfpm package --config nfpm.yaml --packager ${packager} --target $out
              package="$(find "$out" -maxdepth 1 -type f -name "*.${extension}" -print -quit)"
              if [ -n "$package" ]; then
                mv "$package" "$out/${pname}-${packageVersion}-x86_64.${extension}"
              fi
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
        packages.AllDist = pkgs.runCommand "${pname}-${packageVersion}-AllDist" { } ''
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
