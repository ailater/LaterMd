#!/usr/bin/env python3
"""字型量化测量：去装饰后测主体 glyph 的包围盒、重心、间隙。"""
import subprocess, sys, re
from PIL import Image

N = 1024

def strip_decor(svg_text):
    """移除背景、星芒，只留白字glyph，便于测字型本体。"""
    s = svg_text
    # 去掉 <g fill="#FFFFFF"> ... </g> 星芒组
    s = re.sub(r'<g fill="#FFFFFF">.*?</g>', '', s, flags=re.S)
    # 去掉背景底板 + 高光
    s = re.sub(r'<g clip-path="url\(#lmSquare\)">.*?</g>', '', s, flags=re.S)
    s = s.replace('url(#lmGlyph)', '#FFFFFF')
    return s

def mask(path, thresh=128):
    im = Image.open(path).convert("RGBA")
    px = im.load()
    W, H = im.size
    return [[(px[x, y][3] > 60 and sum(px[x, y][:3]) / 3 >= thresh)
             for x in range(W)] for y in range(H)], W, H

def stats(path, label):
    m, W, H = mask(path)
    fg = [(x, y) for y in range(H) for x in range(W) if m[y][x]]
    if not fg:
        print(f"{label}: 无前景"); return
    xs = [p[0] for p in fg]; ys = [p[1] for p in fg]
    bx0, bx1, by0, by1 = min(xs), max(xs), min(ys), max(ys)
    cx = sum(xs) / len(xs); cy = sum(ys) / len(ys)
    gx = (bx0 + bx1) / 2; gy = (by0 + by1) / 2
    cov = len(fg) / (W * H) * 100
    print(f"\n--- {label} ---")
    print(f"  bbox        x[{bx0},{bx1}] y[{by0},{by1}]  尺寸 {bx1-bx0+1} x {by1-by0+1}")
    print(f"  占画布      {(bx1-bx0+1)/W*100:.1f}% x {(by1-by0+1)/H*100:.1f}%")
    print(f"  几何中心    ({gx:.1f}, {gy:.1f})")
    print(f"  视觉重心    ({cx:.1f}, {cy:.1f})   偏移 dx={cx-gx:+.1f} dy={cy-gy:+.1f}")
    print(f"  中心偏移    相对画布中心 dx={cx-W/2:+.1f} dy={cy-H/2:+.1f}")
    print(f"  墨水占比    {cov:.1f}%")

    # V 谷开口高度：从 valley 往上找第一条两笔分离 >=3px 的扫描线
    return dict(bx0=bx0, bx1=bx1, by0=by0, by1=by1, cx=cx, cy=cy, cov=cov, W=W)

def render(svg_path, png_path, size=N):
    subprocess.run(["rsvg-convert", "-w", str(size), "-h", str(size),
                    svg_path, "-o", png_path], check=True)

if __name__ == "__main__":
    for svg in sys.argv[1:]:
        t = open(svg).read()
        tmp = svg.replace(".svg", ".glyphonly.svg")
        open(tmp, "w").write(strip_decor(t))
        png = tmp.replace(".svg", ".png")
        render(tmp, png)
        stats(png, svg.split("/")[-1])
