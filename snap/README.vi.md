# Đóng gói Snap — Snap Store

[English](README.md)

Thư mục này chứa mọi thứ để build **Snap** của **Rust RDP VNC** và đưa lên [Snap Store](https://snapcraft.io).

| File | Vai trò |
|------|---------|
| [`snapcraft.yaml`](snapcraft.yaml) | Định nghĩa snap (tên, plugs, bước build) |
| [`gui/rust-rdp-vnc.desktop`](gui/rust-rdp-vnc.desktop) | Desktop entry trong snap |

Script tiện ích (từ root repo): [`../scripts/publish-snap.sh`](../scripts/publish-snap.sh)

---

## Tổng quan

```
Máy dev                              Snap Store
───────                              ──────────
snapcraft pack  →  file .snap  →  snapcraft upload
                                      │
                                      ├─ edge       (thử nghiệm)
                                      ├─ beta
                                      ├─ candidate
                                      └─ stable     (mặc định công khai)
```

Khác Flathub: bạn **upload gói binary** (`.snap`). Store host gói; user cài bằng `snap install rust-rdp-vnc`.

---

## Chuẩn bị

### 1. Tài khoản

1. Tạo [Ubuntu One](https://login.ubuntu.com/).
2. Vào [snapcraft.io/account](https://snapcraft.io/account), chấp nhận điều khoản developer.
3. (Tuỳ chọn) thiết lập namespace / branding publisher.

### 2. Cài tool trên Linux

```bash
sudo snap install snapcraft --classic

# Nên build trong LXD (sạch, dễ tái lập)
sudo snap install lxd
sudo lxd init --auto
```

### 3. Đăng nhập

```bash
snapcraft login
snapcraft whoami
```

---

## Một lần duy nhất: đăng ký tên snap

Tên snap **toàn cầu, không trùng**.

```bash
./scripts/publish-snap.sh --register

# Hoặc
snapcraft register rust-rdp-vnc
```

Nếu `rust-rdp-vnc` đã bị lấy, đổi `name:` trong `snapcraft.yaml` rồi register tên mới.

---

## Build snap

### Nhanh (script)

```bash
cd /path/to/rust-rdp-vnc
./scripts/publish-snap.sh --build-only
```

### Thủ công

```bash
cd /path/to/rust-rdp-vnc
snapcraft pack
# → rust-rdp-vnc_1.0.1_amd64.snap
```

Lần đầu lâu: tải SDK, compile Rust, IronRDP, openh264, …

### Cài local (trước khi upload)

```bash
sudo snap install --dangerous ./rust-rdp-vnc_*.snap
rust-rdp-vnc
sudo snap remove rust-rdp-vnc
```

`--dangerous` bắt buộc với snap local chưa ký bởi store.

---

## Upload / các kênh (channels)

| Kênh | Dùng khi |
|------|----------|
| **edge** | Test / CI / bản dev |
| **beta** | Thử rộng hơn |
| **candidate** | Sắp ra stable |
| **stable** | Production; mặc định khi `snap install` |

### Upload bằng script

```bash
./scripts/publish-snap.sh              # → edge
./scripts/publish-snap.sh beta
./scripts/publish-snap.sh candidate
./scripts/publish-snap.sh stable
```

### Upload thủ công

```bash
snapcraft upload --release=edge ./rust-rdp-vnc_1.0.1_amd64.snap
```

Hoặc upload rồi promote:

```bash
snapcraft upload ./rust-rdp-vnc_1.0.1_amd64.snap
snapcraft status rust-rdp-vnc
snapcraft release rust-rdp-vnc <revision> edge
snapcraft release rust-rdp-vnc <revision> stable
```

### User cài

```bash
sudo snap install rust-rdp-vnc --edge
sudo snap install rust-rdp-vnc            # sau khi có stable
```

---

## Listing trên Dashboard

Trước khi lên **stable**, hoàn thiện listing tại dashboard snap:

- [ ] Icon 512×512 — `desktop/assets/icon-512.png`
- [ ] Screenshots
- [ ] Tiêu đề, mô tả ngắn/dài
- [ ] Category
- [ ] Contact / website / source
- [ ] License

Listing kém có thể bị từ chối / hạn chế stable.

---

## Hiểu `snapcraft.yaml`

### Định danh

- `name` — tên trên store  
- `version` — tăng khi phát hành  
- `base: core24` — nền Ubuntu 24.04  
- `grade: devel` → đổi `stable` khi chín muồi  
- `confinement: strict` — sandbox; chỉ quyền trong plugs  

### Plugs quan trọng

| Plug | Lý do |
|------|--------|
| `network` | RDP/VNC |
| `opengl` | GUI eframe |
| `home` | mở/lưu file kết nối trong `$HOME` |
| `removable-media` | USB (tuỳ chọn) |
| extension `gnome` | portal, Wayland/X11 |

### Build

- `cargo install --path desktop` trong `override-build`
- Copy icon + desktop vào `meta/gui/`
- Cần mạng lúc build (dependency git Cargo)

### Checklist tăng version

1. Sửa `version:` trong `snapcraft.yaml`  
2. Build lại  
3. Upload  
4. `release` lên channel mong muốn  

---

## Đa kiến trúc (arm64)

Hiện cấu hình **amd64**. Thêm arm64: build trên máy ARM, Launchpad `remote-build`, hoặc mở rộng `platforms:`.

---

## CI (tuỳ chọn)

GitHub Actions: `snapcore/action-build` + token store (`SNAPCRAFT_STORE_CREDENTIALS`).  
**Không** commit credential vào git.

---

## Xử lý lỗi thường gặp

| Lỗi | Hướng xử lý |
|-----|-------------|
| Không có `snapcraft` | `sudo snap install snapcraft --classic` |
| Lỗi LXD | `sudo lxd init --auto`; user trong group `lxd` |
| Tên đã có | Đổi `name:` + register lại |
| App không ra mạng | `snap connections rust-rdp-vnc` |
| Không mở file ngoài home | Giới hạn strict; dùng plug `home` |
| Lỗi build openh264 | Có `nasm`, cmake, libclang trong `build-packages` |
| Upload auth fail | `snapcraft login` lại |

---

## Tài liệu chính thức

- https://snapcraft.io/docs  
- https://snapcraft.io/docs/releasing-to-the-snap-store  
- https://snapcraft.io/docs/channels  

---

## Liên quan trong repo

```
snap/
  snapcraft.yaml
  gui/rust-rdp-vnc.desktop
  README.md
  README.vi.md           ← file này
scripts/publish-snap.sh
desktop/assets/icon*.png
```
