# Packaging

Nix is the single source of truth for release builds. The helper scripts in
this directory are thin wrappers around flake targets; they do not duplicate
packaging logic.

This project has three release tracks:

- `nix`: a native Nix package from `flake.nix`.
- `appimage`: a self-contained x86_64 AppImage for general Linux use.
- `deb-rpm`: `.deb` and `.rpm` packages that install the AppImage payload.

The `.deb` and `.rpm` packages intentionally reuse the AppImage instead of
linking the GTK/libadwaita launcher against each target distro. That keeps the
package-format installers broad while the AppImage carries the portable runtime.

## All Artifacts

Build everything with one command:

```sh
nix build .#AllDist
```

The output contains:

- `nix/hearthstone-linux-gui`: native Nix package output.
- `nix/hearthstone-linux-gui-runtime`: Nix runtime wrapper for the Unity player.
- `appimage/*.AppImage`: portable x86_64 AppImage.
- `deb/*.deb`: Debian package that installs the AppImage payload.
- `rpm/*.rpm`: RPM package that installs the AppImage payload.

## Nix

Build the native package:

```sh
nix build .#default
```

Build only the Nix runtime wrapper used to launch the downloaded Unity player:

```sh
nix build .#runtime
```

## AppImage

Build only the AppImage:

```sh
nix build .#AppImage
```

`packaging/appimage/build.sh` is a convenience wrapper that copies this output
to `dist/`.

## Deb/RPM

Build only one package format:

```sh
nix build .#Deb
nix build .#Rpm
```

`packaging/deb-rpm/build.sh` is a convenience wrapper that copies both package
formats from `.#AllDist` to `dist/packages/`.
