#!/usr/bin/env python3
import os
import sys
import shutil
import struct
import zipfile

def make_ico(png_512, png_128, out_ico):
    images = []
    if png_128 and os.path.exists(png_128):
        with open(png_128, 'rb') as f:
            images.append((128, 128, f.read()))
    if png_512 and os.path.exists(png_512):
        with open(png_512, 'rb') as f:
            images.append((0, 0, f.read()))
    
    header = struct.pack('<HHH', 0, 1, len(images))
    offset = 6 + len(images) * 16
    
    dirs = []
    data_blocks = []
    for w, h, img_data in images:
        dirs.append(struct.pack('<BBBBHHII', w if w < 256 else 0, h if h < 256 else 0, 0, 0, 1, 32, len(img_data), offset))
        data_blocks.append(img_data)
        offset += len(img_data)
        
    with open(out_ico, 'wb') as f:
        f.write(header + b''.join(dirs) + b''.join(data_blocks))

def package_win(target_dir, target_name):
    exe_path = os.path.join(target_dir, "rust-rdp-vnc.exe")
    if not os.path.exists(exe_path):
        print(f"[!] Binary {exe_path} not found, skipping Windows packaging")
        return

    arch_label = "x64"
    if "aarch64" in target_name:
        arch_label = "ARM64"

    # 1. Create icon.ico
    ico_path = os.path.join(target_dir, "icon.ico")
    make_ico("desktop/assets/icon-512.png", "desktop/assets/icon-128.png", ico_path)

    # 2. Generate Inno Setup Script (.iss) for Windows
    iss_content = f"""; Inno Setup Script for Rust RDP VNC
[Setup]
AppName=Rust RDP VNC
AppVersion=1.0.6
DefaultDirName={{autopf}}\\Rust RDP VNC
DefaultGroupName=Rust RDP VNC
UninstallDisplayIcon={{app}}\\icon.ico
Compression=lzma2/max
SolidCompression=yes
OutputDir=.
OutputBaseFilename=Rust-RDP-VNC-Setup-{arch_label}
SetupIconFile=icon.ico

[Files]
Source: "rust-rdp-vnc.exe"; DestDir: "{{app}}"; Flags: ignoreversion
Source: "icon.ico"; DestDir: "{{app}}"; Flags: ignoreversion

[Icons]
Name: "{{autoprograms}}\\Rust RDP VNC"; Filename: "{{app}}\\rust-rdp-vnc.exe"; IconFilename: "{{app}}\\icon.ico"
Name: "{{autodesktop}}\\Rust RDP VNC"; Filename: "{{app}}\\rust-rdp-vnc.exe"; IconFilename: "{{app}}\\icon.ico"

[Run]
Filename: "{{app}}\\rust-rdp-vnc.exe"; Description: "Launch Rust RDP VNC"; Flags: postinstall nowait skipifsilent
"""
    iss_path = os.path.join(target_dir, "installer.iss")
    with open(iss_path, "w") as f:
        f.write(iss_content)

    # 3. Generate PowerShell One-Click Installer (Install-RustRDPVNC.ps1)
    ps1_content = """# PowerShell One-Click Installer for Rust RDP VNC
$ErrorActionPreference = "Stop"

$InstallDir = "$env:LOCALAPPDATA\\Programs\\RustRDPVNC"
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null

$ScriptDir = $PSScriptRoot
Copy-Item -Path "$ScriptDir\\rust-rdp-vnc.exe" -Destination "$InstallDir\\rust-rdp-vnc.exe" -Force
if (Test-Path "$ScriptDir\\icon.ico") {
    Copy-Item -Path "$ScriptDir\\icon.ico" -Destination "$InstallDir\\icon.ico" -Force
}

# Create Desktop Shortcut
$WshShell = New-Object -ComObject WScript.Shell
$DesktopShortcut = $WshShell.CreateShortcut("$env:USERPROFILE\\Desktop\\Rust RDP VNC.lnk")
$DesktopShortcut.TargetPath = "$InstallDir\\rust-rdp-vnc.exe"
$DesktopShortcut.IconLocation = "$InstallDir\\icon.ico"
$DesktopShortcut.Save()

# Create Start Menu Shortcut
$StartMenuDir = "$env:APPDATA\\Microsoft\\Windows\\Start Menu\\Programs"
$StartShortcut = $WshShell.CreateShortcut("$StartMenuDir\\Rust RDP VNC.lnk")
$StartShortcut.TargetPath = "$InstallDir\\rust-rdp-vnc.exe"
$StartShortcut.IconLocation = "$InstallDir\\icon.ico"
$StartShortcut.Save()

Write-Host "✔ Rust RDP VNC installed successfully to $InstallDir" -ForegroundColor Green
Write-Host "✔ Desktop and Start Menu shortcuts created!" -ForegroundColor Green
"""
    ps1_path = os.path.join(target_dir, "Install-RustRDPVNC.ps1")
    with open(ps1_path, "w") as f:
        f.write(ps1_content)

    # 4. Generate README.txt
    readme_content = """Rust RDP VNC - Windows Package
===============================

Features:
- GUI mode without CMD console window
- Embedded high-resolution icon

Installation options:
1. One-Click PowerShell Installer: Right-click 'Install-RustRDPVNC.ps1' -> Run with PowerShell.
2. Inno Setup: Compile 'installer.iss' with Inno Setup on Windows to create a Setup.exe installer.
3. Portable: Run 'rust-rdp-vnc.exe' directly.
"""
    readme_path = os.path.join(target_dir, "README.txt")
    with open(readme_path, "w") as f:
        f.write(readme_content)

    # 5. Zip specific distribution files into Zip archive
    zip_name = f"Rust-RDP-VNC-Windows-{arch_label}.zip"
    zip_path = os.path.join(target_dir, zip_name)
    files_to_zip = [
        ("rust-rdp-vnc.exe", exe_path),
        ("icon.ico", ico_path),
        ("Install-RustRDPVNC.ps1", ps1_path),
        ("installer.iss", iss_path),
        ("README.txt", readme_path),
    ]
    with zipfile.ZipFile(zip_path, 'w', zipfile.ZIP_DEFLATED) as zf:
        for arcname, fpath in files_to_zip:
            if os.path.exists(fpath):
                zf.write(fpath, arcname)

    print(f"  ✓ Created Windows installer & zip package: {zip_path}")

if __name__ == "__main__":
    if len(sys.argv) < 3:
        print("Usage: package_win.py <target_dir> <target_name>")
        sys.exit(1)
    package_win(sys.argv[1], sys.argv[2])
