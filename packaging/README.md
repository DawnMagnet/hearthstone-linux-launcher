# Packaging

Nix is the source of truth for release builds.

This project exposes two build tracks:

- `nix`: the native Nix package from `flake.nix`.
- `appimage`: a portable x86_64 AppImage defined in `nix/appimage.nix`.

## Nix

Build the native package:

```sh
nix build .#default
```

Build the Nix runtime wrapper used to launch the downloaded Unity player:

```sh
nix build .#runtime
```

## AppImage

Build the AppImage:

```sh
nix build .#AppImage
```

`packaging/appimage/build.sh` is a convenience wrapper that copies the AppImage
to `dist/`.
