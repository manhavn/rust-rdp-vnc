# Snap packaging — Snap Store

[Tiếng Việt](README.vi.md)

This directory contains everything needed to build a **Snap** of **Rust RDP VNC** and publish it to the [Snap Store](https://snapcraft.io).

| File | Purpose |
|------|---------|
| [`snapcraft.yaml`](snapcraft.yaml) | Snap definition (name, plugs, build steps) |
| [`gui/rust-rdp-vnc.desktop`](gui/rust-rdp-vnc.desktop) | Desktop entry installed into the snap |

Convenience script (from repo root): [`../scripts/publish-snap.sh`](../scripts/publish-snap.sh)

---

## Overview

```
Developer machine                    Snap Store
─────────────────                    ──────────
snapcraft pack  →  .snap file  →  snapcraft upload
                                      │
                                      ├─ edge       (testing)
                                      ├─ beta
                                      ├─ candidate
                                      └─ stable     (public default)
```

Unlike Flathub, you **upload a built binary package** (`.snap`). The store hosts it; users install with `snap install rust-rdp-vnc`.

---

## Prerequisites

### 1. Accounts

1. Create an [Ubuntu One](https://login.ubuntu.com/) account.
2. Open [snapcraft.io/account](https://snapcraft.io/account) and accept the developer agreement.
3. Optionally set up a publisher namespace / branding.

### 2. Tools on your Linux machine

```bash
sudo snap install snapcraft --classic

# Recommended: build inside LXD (clean, reproducible)
sudo snap install lxd
sudo lxd init --auto   # accept defaults if unsure
```

### 3. Login

```bash
snapcraft login
# browser / email verification as prompted
snapcraft whoami
```

---

## One-time: register the snap name

Snap names are **global and unique**.

```bash
# From repo root
./scripts/publish-snap.sh --register

# Or manually
snapcraft register rust-rdp-vnc
```

If `rust-rdp-vnc` is taken, change `name:` in `snapcraft.yaml` and register the new name.

---

## Build the snap

### Quick (script)

```bash
cd /path/to/rust-rdp-vnc
./scripts/publish-snap.sh --build-only
```

### Manual

```bash
cd /path/to/rust-rdp-vnc
snapcraft pack
# → rust-rdp-vnc_1.0.1_amd64.snap (version from snapcraft.yaml)
```

First build is slow: downloads SDK, compiles Rust, IronRDP, openh264, etc.

### Install locally (before uploading)

```bash
sudo snap install --dangerous ./rust-rdp-vnc_*.snap
rust-rdp-vnc
# Remove when done testing
sudo snap remove rust-rdp-vnc
```

`--dangerous` is required for local snaps that are not signed by the store.

---

## Upload / release channels

| Channel | Typical use |
|---------|-------------|
| **edge** | CI / every commit / early testing |
| **beta** | Wider testing |
| **candidate** | Release candidate |
| **stable** | Production; default for `snap install` |

### Upload with script

```bash
./scripts/publish-snap.sh              # upload to edge
./scripts/publish-snap.sh edge
./scripts/publish-snap.sh beta
./scripts/publish-snap.sh candidate
./scripts/publish-snap.sh stable       # only when ready for everyone
```

### Upload manually

```bash
snapcraft upload --release=edge ./rust-rdp-vnc_1.0.1_amd64.snap
```

Or upload without releasing, then promote:

```bash
snapcraft upload ./rust-rdp-vnc_1.0.1_amd64.snap
snapcraft status rust-rdp-vnc
snapcraft release rust-rdp-vnc <revision> edge
snapcraft release rust-rdp-vnc <revision> stable
```

### Users install

```bash
sudo snap install rust-rdp-vnc --edge     # while only on edge
sudo snap install rust-rdp-vnc            # after stable
```

---

## Store listing (Dashboard)

Before promoting to **stable**, complete the listing at  
[https://snapcraft.io/rust-rdp-vnc](https://snapcraft.io) → your snap → **Listing**:

- [ ] Icon (512×512 PNG) — use `desktop/assets/icon-512.png`
- [ ] Screenshots (desktop session, connection form)
- [ ] Title, summary, full description
- [ ] Category (e.g. Productivity / Utilities / Networking)
- [ ] Contact / website / source code URL
- [ ] License field aligned with the project

Poor listings delay or block stable visibility.

---

## Understanding `snapcraft.yaml`

### Identity

- `name: rust-rdp-vnc` — store package name  
- `version` — bump when releasing  
- `base: core24` — Ubuntu 24.04 base  
- `grade: devel` — use `stable` when the package is production-ready  
- `confinement: strict` — sandbox; access only via declared plugs  

### App + plugs

| Plug | Why |
|------|-----|
| `network` / `network-bind` | RDP/VNC TCP |
| `opengl` | eframe / GPU |
| `home` | open/save `.rdp` / `.vnc` under `$HOME` |
| `removable-media` | optional USB/SD paths |
| `gnome` extension | desktop portals, theming, Wayland/X11 helpers |

### Build

- `plugin: rust` + `override-build` runs `cargo install --path desktop`
- Copies icon + `.desktop` into `meta/gui/`
- Host needs network during build (Cargo git deps)

### Version bump checklist

1. Update `version:` in `snapcraft.yaml`  
2. Optionally align `desktop` / docs version strings  
3. Rebuild and upload  
4. `snapcraft release … stable` if applicable  

---

## Architecture (multi-arch)

Current config builds **amd64** only. To add arm64 (e.g. Raspberry Pi, ARM laptops):

1. Build on arm64 hardware or Launchpad remote build  
2. Extend `platforms:` in `snapcraft.yaml`  
3. Or use: `snapcraft remote-build` (Launchpad account)

---

## CI ideas (optional)

GitHub Actions can:

1. Run `snapcraft pack` with `snapcore/action-build`  
2. Upload with a Snap Store export token (`SNAPCRAFT_STORE_CREDENTIALS`)  

Never commit store credentials to git.

---

## Troubleshooting

| Problem | What to try |
|---------|-------------|
| `snapcraft: command not found` | `sudo snap install snapcraft --classic` |
| LXD errors | `sudo lxd init --auto`; ensure user in `lxd` group |
| Name already registered | Change `name:` and re-register |
| App starts but no network | Check plugs; reconnect: `snap connections rust-rdp-vnc` |
| Cannot open files outside home | Strict confinement; grant `home` / `removable-media` or use classic (not recommended) |
| Build fails on openh264 / llvm | Ensure `nasm`, `libclang-dev`, cmake in `build-packages` |
| Upload auth failed | `snapcraft logout` then `snapcraft login` |

---

## Official docs

- [Snapcraft documentation](https://snapcraft.io/docs)
- [Releasing to the Snap Store](https://snapcraft.io/docs/releasing-to-the-snap-store)
- [Snap confinement](https://snapcraft.io/docs/snap-confinement)
- [Channels and tracks](https://snapcraft.io/docs/channels)

---

## Related paths in this repo

```
snap/
  snapcraft.yaml
  gui/rust-rdp-vnc.desktop
  README.md              ← this file
  README.vi.md
scripts/publish-snap.sh
desktop/assets/icon*.png
```
