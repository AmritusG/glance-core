"""tools/c3-look-stats.py <png>... — numbers for the side-by-side look review (C3): for each window
capture (2x, title bar included), the rows from the bottom up are grouped into runs of the same
background grey (the most common colour across the whole row: the background, since controls and text
take the minority of a row), giving each band's height in points and its grey. The captures themselves are for
Amrit's eyes (G2); these numbers are what the core can check."""
import sys
from PIL import Image

for path in sys.argv[1:]:
    im = Image.open(path).convert("RGB")
    w, h = im.size
    x0, x1 = 0, w
    def row_grey(y):
        px = [im.getpixel((x, y)) for x in range(x0, x1, 2)]
        # The most common value in the strip (controls and text are the minority).
        best = max(set(px), key=px.count)
        return best, px.count(best) / len(px)
    bands, y = [], h - 1
    while y >= 0 and len(bands) < 8:
        g, share = row_grey(y)
        start = y
        while y >= 0 and row_grey(y)[0] == g:
            y -= 1
        bands.append((g, (start - y) / 2.0, share))
    print(path.split("/")[-1], f"{w // 2}x{h // 2} pt")
    for g, height, share in bands:
        print(f"    band from bottom: {height:6.1f} pt  rgb {g}  (strip share {share:.0%})")
