#!/usr/bin/env python3
"""小尺寸存活审计：A 的三角字腔、V 谷开口、D 碗内白心在各尺寸是否还活着。"""
import subprocess, sys, os
from PIL import Image

def render(svg, png, size):
    subprocess.run(["rsvg-convert", "-w", str(size), "-h", str(size), svg, "-o", png],
                   check=True, capture_output=True)

def load_mask(png, thresh=170):
    im = Image.open(png).convert("RGB")
    px = im.load(); W, H = im.size
    return [[sum(px[x, y]) / 3 >= thresh for x in range(W)] for y in range(H)], W, H

def find_runs(row):
    runs, start = [], None
    for i, v in enumerate(row):
        if v and start is None: start = i
        elif not v and start is not None:
            runs.append((start, i - 1)); start = None
    if start is not None: runs.append((start, len(row) - 1))
    return runs

def gaps(runs):
    return [runs[i + 1][0] - runs[i][1] - 1 for i in range(len(runs) - 1)]

def audit(svg, sizes=(512, 192, 96, 64, 48)):
    print(f"\n{'='*64}\n{os.path.basename(svg)}\n{'='*64}")
    print(f"{'size':>5} {'run数':>5} {'最小间隙':>8} {'最大间隙':>8} {'ink%':>6}  判定")
    base, _ = os.path.splitext(svg)
    for s in sizes:
        png = f"{base}.audit{s}.png"
        render(svg, png, s)
        m, W, H = load_mask(png)
        allgaps, ink = [], 0
        maxrun = 0
        for y in range(H):
            row = m[y]
            r = find_runs(row)
            allgaps += gaps(r)
            maxrun = max(maxrun, len(r))
            ink += sum(1 for v in row if v)
        ming = min(allgaps) if allgaps else 0
        maxg = max(allgaps) if allgaps else 0
        inkpct = ink / (W * H) * 100
        # 判定：48px 下至少要有 >=1px 的间隙，且不能只有 1 段（糊成一坨）
        ok = ming >= 1 and maxrun >= 2
        print(f"{s:>5} {maxrun:>5} {ming:>8} {maxg:>8} {inkpct:>6.1f}  "
              f"{'OK' if ok else 'FAIL 糊死'}")
    print()

if __name__ == "__main__":
    for s in sys.argv[1:]:
        audit(s)
