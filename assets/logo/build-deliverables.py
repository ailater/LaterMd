#!/usr/bin/env python3
"""LaterMD 图标交付流水线：多尺寸 PNG + 各平台原生格式 + 变体 SVG。"""
import subprocess, os, shutil, re

ROOT = "/home/data/www/LaterMD/assets/logo"
SRC = f"{ROOT}/latermd-icon.svg"
DIST = f"{ROOT}/deliverables"
PNG = f"{DIST}/png"

SIZES = [1024, 512, 256, 192, 128, 96, 64, 48, 32, 16]

def run(cmd):
    subprocess.run(cmd, check=True, capture_output=True)

def render(src, out, w, h=None, bg=None):
    h = h or w
    c = ["rsvg-convert", "-w", str(w), "-h", str(h)]
    if bg: c += ["--background-color", bg]
    c += [src, "-o", out]
    run(c)

def variant(src_text, out_path, mode):
    """生成变体 SVG：mono-white / mono-black / flat / no-star"""
    s = src_text
    if mode == "mono-white":
        s = re.sub(r'<g clip-path="url\(#lmSquare\)">.*?</g>', "", s, flags=re.S)
        s = s.replace('url(#lmGlyph)', '#FFFFFF')
        s = re.sub(r'<g fill="#FFFFFF">\s*<use href="#lmSparkle".*?</g>', "", s, flags=re.S)
    elif mode == "mono-black":
        s = re.sub(r'<g clip-path="url\(#lmSquare\)">.*?</g>', "", s, flags=re.S)
        s = s.replace('url(#lmGlyph)', '#0A1E5C')
        s = re.sub(r'<g fill="#FFFFFF">\s*<use href="#lmSparkle".*?</g>', "", s, flags=re.S)
    elif mode == "flat-nostar":
        s = re.sub(r'<g fill="#FFFFFF">\s*<use href="#lmSparkle".*?</g>', "", s, flags=re.S)
    open(out_path, "w").write(s)

def main():
    shutil.rmtree(DIST, ignore_errors=True)
    os.makedirs(PNG, exist_ok=True)
    src = open(SRC).read()

    # --- 变体 SVG ---
    var_dir = f"{DIST}/svg"
    os.makedirs(var_dir, exist_ok=True)
    shutil.copy(SRC, f"{var_dir}/latermd-icon.svg")
    for mode, name in [("mono-white", "latermd-icon-mono-white.svg"),
                       ("mono-black", "latermd-icon-mono-black.svg"),
                       ("flat-nostar", "latermd-icon-no-star.svg")]:
        variant(src, f"{var_dir}/{name}", mode)
    print("=== 变体 SVG ===")
    for f in sorted(os.listdir(var_dir)):
        print(f"  {f}")

    # --- PNG 多尺寸 ---
    print("\n=== PNG 多尺寸 ===")
    for s in SIZES:
        p = f"{PNG}/icon-{s}.png"
        render(SRC, p, s)
        print(f"  {s:>4}px  {os.path.getsize(p)/1024:>6.1f} KB")

    # --- macOS .iconset ---
    print("\n=== macOS AppIcon.iconset ===")
    iconset = f"{DIST}/macOS/AppIcon.iconset"
    os.makedirs(iconset, exist_ok=True)
    mac = [(16,"icon_16x16"),(32,"icon_16x16@2x"),(32,"icon_32x32"),(64,"icon_32x32@2x"),
           (128,"icon_128x128"),(256,"icon_128x128@2x"),(256,"icon_256x256"),
           (512,"icon_256x256@2x"),(512,"icon_512x512"),(1024,"icon_512x512@2x")]
    for px, nm in mac:
        render(SRC, f"{iconset}/{nm}.png", px)
    print(f"  {len(mac)} 个文件 -> macOS/AppIcon.iconset")
    print("  合成命令: iconutil -c icns macOS/AppIcon.iconset -o macOS/AppIcon.icns")

    # --- Windows .ico ---
    print("\n=== Windows .ico ===")
    win = f"{DIST}/windows"
    os.makedirs(win, exist_ok=True)
    ico_src = [f"{PNG}/icon-{s}.png" for s in (16,32,48,64,128,256)]
    run(["convert"] + ico_src + [f"{win}/latermd.ico"])
    print(f"  latermd.ico  {os.path.getsize(f'{win}/latermd.ico')/1024:.1f} KB")

    # --- favicon ---
    print("\n=== favicon ===")
    fav = f"{DIST}/web"
    os.makedirs(fav, exist_ok=True)
    run(["convert"] + [f"{PNG}/icon-{s}.png" for s in (16,32,48)] + [f"{fav}/favicon.ico"])
    render(SRC, f"{fav}/favicon.svg", 32)
    render(SRC, f"{fav}/apple-touch-icon.png", 180)
    print(f"  favicon.ico {os.path.getsize(f'{fav}/favicon.ico')/1024:.1f} KB")
    print(f"  apple-touch-icon.png 180px")

    # --- Android ---
    print("\n=== Android ===")
    andr = f"{DIST}/android"
    for dpi, px in [("mdpi",48),("hdpi",72),("xhdpi",96),("xxhdpi",144),("xxxhdpi",192)]:
        d = f"{andr}/res/mipmap-{dpi}"
        os.makedirs(d, exist_ok=True)
        render(SRC, f"{d}/ic_launcher.png", px)
    print("  mipmap mdpi~xxxhdpi 已生成")

    print(f"\n全部产出 -> {DIST}")

if __name__ == "__main__":
    main()
