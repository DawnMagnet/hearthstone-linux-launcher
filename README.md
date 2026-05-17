# hearthstone-linux-gui

<p align="center">
  <img src="docs/images/readme-hero.svg" alt="hearthstone-linux-gui desktop launcher preview" width="920">
</p>

<p align="center">
  <a href="README.cn.md">中文说明</a>
  ·
  <a href="https://github.com/DawnMagnet/hearthstone-linux-launcher/releases/latest">Latest release</a>
</p>

<p align="center">
  <img alt="Linux x86_64" src="https://img.shields.io/badge/Linux-x86__64-2f6f73?style=flat-square">
  <img alt="GTK4" src="https://img.shields.io/badge/GTK-4-4a86cf?style=flat-square">
  <img alt="Rust" src="https://img.shields.io/badge/Rust-native-b94700?style=flat-square">
  <img alt="Packages" src="https://img.shields.io/badge/AppImage%20%7C%20DEB%20%7C%20RPM%20%7C%20Nix-ready-6750a4?style=flat-square">
</p>

**hearthstone-linux-gui** is a native GTK4 desktop manager for installing,
updating, logging into, and launching Hearthstone on Linux. It is migrated from
the original [`hearthstone-linux`](https://github.com/0xf4b1/hearthstone-linux)
project, but the old script-driven workflow has been replaced by a packaged
Rust application with a graphical interface.

No terminal workflow is required for normal users. Download a release, install
or open it with your desktop environment, click **Install / Update**, click
**Login**, then click **Play**.

## What Changed From The Original Project

The original `hearthstone-linux` proved that Hearthstone can run on Linux by
combining Blizzard's official game files with Unity's Linux runtime. This
project keeps that idea, but turns it into a desktop application that is easier
to distribute and maintain.

<p align="center">
  <img src="docs/images/readme-flow.svg" alt="Old script workflow replaced by a GTK4 desktop workflow" width="860">
</p>

| Original `hearthstone-linux` | This project |
| --- | --- |
| Script-oriented setup | GTK4/libadwaita desktop application |
| Manual command-line flow | Button-driven install, login, update, and launch |
| Python/Bash toolchain expected by users | Packaged runtime; no Python or Bash environment needed for normal use |
| External `keg` downloader workflow | Native Rust NGDP downloader with cache and verification |
| Distro-specific setup pain | AppImage, DEB, RPM, and native Nix package outputs |

## Highlights

- **One-click desktop experience**: install, update, login, and launch from a
  GTK4 window.
- **No command-line requirement**: release builds are meant for graphical
  installation and daily use.
- **No Python/Bash runtime requirement**: the launcher is a native Rust binary
  packaged with the libraries it needs.
- **Cross-distribution Linux packaging**: use the same project on NixOS,
  Debian, Ubuntu, Fedora, and other x86_64 Linux distributions.
- **Portable AppImage**: a self-contained build for broad Linux compatibility.
- **Native Nix package**: Nix users can consume a standard package output with
  desktop integration.
- **DEB/RPM installers**: package-manager friendly builds for common desktop
  distributions.
- **Resumable downloads**: Unity runtime downloads can continue from a partial
  file after interruption.
- **Cached game data**: downloaded NGDP content is cached and verified to avoid
  unnecessary network work.
- **No Steam dependency**: the AppImage carries the portable GTK/runtime layer;
  the game itself is launched with the project's own runtime handling, not
  `steam-run`.

## Packages

<p align="center">
  <img src="docs/images/readme-packages.svg" alt="Release package formats: AppImage, DEB, RPM, and Nix" width="860">
</p>

| Package | Best for | User experience |
| --- | --- | --- |
| **AppImage** | Any x86_64 Linux desktop, including NixOS | Download and open the application directly |
| **DEB** | Debian, Ubuntu, Linux Mint, Pop!_OS, and related systems | Open with the graphical software installer |
| **RPM** | Fedora, RHEL-compatible, openSUSE-style workflows | Open with the graphical software installer |
| **Nix** | NixOS and Nix package users | Native package output with desktop file and launcher |

Release builds are produced from the unified Nix build pipeline so every format
comes from the same source, version, and dependency graph.

## Installation For Users

1. Open the
   [latest release page](https://github.com/DawnMagnet/hearthstone-linux-launcher/releases/latest).
2. Download the package that matches your system.
3. Open it with your desktop environment:
   - AppImage: open the downloaded AppImage. If your file manager asks for it,
     enable "Allow executing file as program" in file properties.
   - DEB/RPM: double-click the package and install it with your graphical
     software center.
   - Nix/NixOS: install the native Nix package through your normal Nix workflow.
4. Launch **hearthstone-linux-gui** from the application menu.

After the app opens, the normal flow is:

1. Choose your **Region** and **Locale**.
2. Click **Install / Update**.
3. Click **Login** and complete Battle.net login in your browser.
4. Return to the app and click **Play**.

The app stores user data under standard XDG locations in your home directory.
Interrupted Unity downloads are resumed automatically, and already downloaded
game data is reused when possible.

## How It Works

Hearthstone ships official data through Blizzard's NGDP distribution system.
The game is built with Unity, and the Linux Unity player can run the game data
after the platform layout is adapted.

This launcher automates that process:

1. Downloads the official Hearthstone game data for the selected region and
   locale.
2. Verifies and caches downloaded content.
3. Transforms the macOS-style payload into a Linux-ready layout.
4. Detects the required Unity version and installs the matching Linux Unity
   player.
5. Installs compatibility files and configuration needed by the game.
6. Registers the login callback handler and stores the encrypted token locally.
7. Launches the game through a controlled Linux runtime environment.

No proprietary Hearthstone files are stored in this repository or shipped in
the launcher packages. The application retrieves official files from their
upstream distribution endpoints during installation.

## Status And Limitations

- Target architecture: **x86_64 Linux**.
- The game client runs, but this project is unofficial.
- The in-game shop may remain unavailable depending on upstream behavior.
- Use at your own risk. This project is not affiliated with Blizzard
  Entertainment.

## Troubleshooting

| Symptom | What to try |
| --- | --- |
| The app says **Login Required** | Click **Login** again and finish the browser flow. |
| Install was interrupted | Click **Install / Update** again; resumable and cached downloads will be reused. |
| The game does not launch after an update | Click **Install / Update** once to repair Unity/runtime files. |
| A package opens but does not start on an unusual distro | Try the AppImage release, which carries the widest runtime set. |

Release builds default to INFO-level logging. Detailed diagnostic logs can be
enabled by developers with the standard `RUST_LOG` environment variable when
debugging locally.

## Legal Notice

Hearthstone is (C) Blizzard Entertainment, Inc. This project is an unofficial
compatibility launcher and is not endorsed by or affiliated with Blizzard
Entertainment. No proprietary Hearthstone game assets are distributed in this
repository.
