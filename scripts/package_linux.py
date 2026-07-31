#!/usr/bin/env python3
import os
import sys
import shutil
import subprocess

def package_linux(target_dir, target_name):
    binary_path = os.path.abspath(os.path.join(target_dir, "rust-rdp-vnc"))
    if not os.path.exists(binary_path):
        print(f"[!] Binary {binary_path} not found, skipping Linux packaging")
        return

    arch = "amd64"
    rpm_arch = "x86_64"
    if "aarch64" in target_name:
        arch = "arm64"
        rpm_arch = "aarch64"

    pkg_name = f"rust-rdp-vnc_1.0.5_{target_name}"
    pkg_dir = os.path.abspath(os.path.join(target_dir, pkg_name))
    if os.path.exists(pkg_dir):
        shutil.rmtree(pkg_dir)

    # 1. Directory hierarchy
    bin_dir = os.path.join(pkg_dir, "usr", "bin")
    apps_dir = os.path.join(pkg_dir, "usr", "share", "applications")
    icon_512_dir = os.path.join(pkg_dir, "usr", "share", "icons", "hicolor", "512x512", "apps")
    icon_128_dir = os.path.join(pkg_dir, "usr", "share", "icons", "hicolor", "128x128", "apps")
    debian_dir = os.path.join(pkg_dir, "DEBIAN")

    os.makedirs(bin_dir, exist_ok=True)
    os.makedirs(apps_dir, exist_ok=True)
    os.makedirs(icon_512_dir, exist_ok=True)
    os.makedirs(icon_128_dir, exist_ok=True)
    os.makedirs(debian_dir, exist_ok=True)

    # 2. Copy binary
    dest_bin = os.path.join(bin_dir, "rust-rdp-vnc")
    shutil.copy2(binary_path, dest_bin)
    os.chmod(dest_bin, 0o755)

    # 3. Copy desktop entry
    desktop_src = os.path.abspath("desktop/assets/io.github.manhavn.rust-rdp-vnc.desktop")
    if os.path.exists(desktop_src):
        shutil.copy2(desktop_src, os.path.join(apps_dir, "io.github.manhavn.rust-rdp-vnc.desktop"))

    # 4. Copy icons
    icon_512_src = os.path.abspath("desktop/assets/icon-512.png")
    if os.path.exists(icon_512_src):
        shutil.copy2(icon_512_src, os.path.join(icon_512_dir, "rust-rdp-vnc.png"))
        shutil.copy2(icon_512_src, os.path.join(icon_512_dir, "io.github.manhavn.rust-rdp-vnc.png"))

    icon_128_src = os.path.abspath("desktop/assets/icon-128.png")
    if os.path.exists(icon_128_src):
        shutil.copy2(icon_128_src, os.path.join(icon_128_dir, "rust-rdp-vnc.png"))
        shutil.copy2(icon_128_src, os.path.join(icon_128_dir, "io.github.manhavn.rust-rdp-vnc.png"))

    # 5. DEBIAN/control
    control_content = f"""Package: rust-rdp-vnc
Version: 1.0.5
Section: utils
Priority: optional
Architecture: {arch}
Maintainer: manhavn <manhavn@github.com>
Description: Rust RDP VNC Desktop Client
 High-performance RDP and VNC desktop client powered by Rust core and egui GUI.
"""
    with open(os.path.join(debian_dir, "control"), "w") as f:
        f.write(control_content)

    # 6. Build .deb
    deb_file = os.path.abspath(os.path.join(target_dir, f"rust-rdp-vnc_1.0.5_{target_name}.deb"))
    res = subprocess.run(["dpkg-deb", "-b", pkg_dir, deb_file], capture_output=True, text=True)
    if res.returncode == 0:
        print(f"  ✓ Created Linux .deb package: {deb_file}")
    else:
        print(f"  [!] Failed to create .deb: {res.stderr}")

    # 7. Build .rpm using cargo-generate-rpm or rpmbuild
    rpm_file = os.path.abspath(os.path.join(target_dir, f"rust-rdp-vnc-1.0.5-1.{target_name}.rpm"))
    cargo_rpm = shutil.which("cargo-generate-rpm") or os.path.expanduser("~/.cargo/bin/cargo-generate-rpm")

    if os.path.exists(cargo_rpm):
        rel_binary = os.path.relpath(binary_path, "desktop")
        meta_assets = (
            f'assets = ['
            f'{{ source = "{rel_binary}", dest = "/usr/bin/rust-rdp-vnc", mode = "755" }}, '
            f'{{ source = "assets/io.github.manhavn.rust-rdp-vnc.desktop", dest = "/usr/share/applications/io.github.manhavn.rust-rdp-vnc.desktop", mode = "644" }}, '
            f'{{ source = "assets/icon-512.png", dest = "/usr/share/icons/hicolor/512x512/apps/rust-rdp-vnc.png", mode = "644" }}, '
            f'{{ source = "assets/icon-128.png", dest = "/usr/share/icons/hicolor/128x128/apps/rust-rdp-vnc.png", mode = "644" }}'
            f']'
        )
        cmd = [
            cargo_rpm,
            "-s", meta_assets,
            "--target", target_name,
            "--target-dir", os.path.abspath("target"),
            "-a", rpm_arch,
            "--auto-req", "disabled",
            "-o", rpm_file
        ]
        res_rpm = subprocess.run(cmd, cwd="desktop", capture_output=True, text=True)
        if res_rpm.returncode == 0 and os.path.exists(rpm_file):
            print(f"  ✓ Created Linux .rpm package: {rpm_file}")
        else:
            print(f"  [!] Failed to create .rpm: {res_rpm.stderr}")
    else:
        print("  [!] cargo-generate-rpm not found, skipping .rpm packaging")

    # 8. Create tar.gz archive
    tar_base = os.path.abspath(os.path.join(target_dir, f"rust-rdp-vnc-1.0.5-{target_name}"))
    shutil.make_archive(tar_base, "gztar", pkg_dir, "usr")
    print(f"  ✓ Created Linux .tar.gz distribution: {tar_base}.tar.gz")

    # Clean up build dir
    shutil.rmtree(pkg_dir, ignore_errors=True)

if __name__ == "__main__":
    if len(sys.argv) < 3:
        print("Usage: package_linux.py <target_dir> <target_name>")
        sys.exit(1)
    package_linux(sys.argv[1], sys.argv[2])
