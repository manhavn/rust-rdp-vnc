#!/usr/bin/env python3
import os
import sys
import shutil
import struct
from pyfatfs.PyFatFS import PyFatFS

def make_icns(png_512, png_128, out_icns):
    chunks = []
    if png_128 and os.path.exists(png_128):
        with open(png_128, 'rb') as f:
            d = f.read()
        chunks.append(b'ic07' + struct.pack('>I', len(d) + 8) + d)
    if png_512 and os.path.exists(png_512):
        with open(png_512, 'rb') as f:
            d = f.read()
        chunks.append(b'ic09' + struct.pack('>I', len(d) + 8) + d)

    body = b''.join(chunks)
    total = len(body) + 8
    with open(out_icns, 'wb') as f:
        f.write(b'icns' + struct.pack('>I', total) + body)

def package_target(target_dir, target_name):
    binary_path = os.path.join(target_dir, "rust-rdp-vnc")
    if not os.path.exists(binary_path):
        print(f"[!] Binary {binary_path} not found, skipping macOS bundle")
        return

    app_name = "Rust RDP VNC.app"
    app_dir = os.path.join(target_dir, app_name)
    contents_dir = os.path.join(app_dir, "Contents")
    macos_dir = os.path.join(contents_dir, "MacOS")
    resources_dir = os.path.join(contents_dir, "Resources")

    os.makedirs(macos_dir, exist_ok=True)
    os.makedirs(resources_dir, exist_ok=True)

    dest_bin = os.path.join(macos_dir, "rust-rdp-vnc")
    shutil.copy2(binary_path, dest_bin)
    os.chmod(dest_bin, 0o755)

    icns_path = os.path.join(resources_dir, "AppIcon.icns")
    make_icns("desktop/assets/icon-512.png", "desktop/assets/icon-128.png", icns_path)

    info_plist = """<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleExecutable</key>
    <string>rust-rdp-vnc</string>
    <key>CFBundleIconFile</key>
    <string>AppIcon</string>
    <key>CFBundleIdentifier</key>
    <string>io.github.manhavn.rust-rdp-vnc</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>Rust RDP VNC</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>1.0.6</string>
    <key>CFBundleVersion</key>
    <string>1.0.6</string>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
"""
    with open(os.path.join(contents_dir, "Info.plist"), "w") as f:
        f.write(info_plist)

    print(f"  ✓ Created macOS .app bundle: {app_dir}")

    dmg_filename = f"Rust-RDP-VNC-{target_name}.dmg"
    dmg_path = os.path.join(target_dir, dmg_filename)
    if os.path.exists(dmg_path):
        os.remove(dmg_path)

    total_size = 0
    for root, dirs, files in os.walk(app_dir):
        for file in files:
            total_size += os.path.getsize(os.path.join(root, file))
    size_kb = int((total_size / 1024) * 1.5) + 8192

    cmd = f'/usr/sbin/mkfs.fat -C -F 32 -n "RustRDPVNC" "{dmg_path}" {size_kb} >/dev/null 2>&1'
    os.system(cmd)

    fs = PyFatFS(filename=dmg_path)
    for root, dirs, files in os.walk(app_dir):
        rel_root = os.path.relpath(root, target_dir)
        fs.makedirs(rel_root)
        for file in files:
            src_file = os.path.join(root, file)
            rel_file = os.path.relpath(src_file, target_dir)
            with open(src_file, "rb") as sf:
                fs.writebytes(rel_file, sf.read())
    fs.close()
    print(f"  ✓ Created macOS .dmg package: {dmg_path}")

    zip_base = os.path.join(target_dir, f"Rust-RDP-VNC-{target_name}")
    shutil.make_archive(zip_base, "zip", target_dir, app_name)
    print(f"  ✓ Created macOS .zip package: {zip_base}.zip")

if __name__ == "__main__":
    if len(sys.argv) < 3:
        print("Usage: package_mac.py <target_dir> <target_name>")
        sys.exit(1)
    package_target(sys.argv[1], sys.argv[2])
