"""analyse.py <out-dir> <label>: frames a second, frame interval and CPU while visible / covered / visible again."""
import csv, statistics, sys
out, label = sys.argv[1], sys.argv[2]
ev = dict(l.split() for l in open(f"{out}/{label}-cover.txt") if l.strip())
seen, on, off = (int(ev[k]) for k in ("seen", "on", "off"))
on_s, off_s = (on - seen) / 1000, (off - seen) / 1000
rows = [(float(r["t_ms"]) / 1000, float(r["present_interval_ms"])) for r in csv.DictReader(open(f"{out}/{label}-frames.csv"))]
def secs(t):
    p = t.split(":"); return float(p[0]) * 60 + float(p[1]) if len(p) == 2 else float(p[0]) * 3600 + float(p[1]) * 60 + float(p[2])
cpu = [(int(a) / 1000 - seen / 1000, secs(b)) for a, b in (l.split() for l in open(f"{out}/{label}-cpu.txt") if len(l.split()) == 2)]
def cpu_pct(a, b):
    s = [c for c in cpu if a <= c[0] <= b]
    return 100 * (s[-1][1] - s[0][1]) / (s[-1][0] - s[0][0]) if len(s) >= 2 else float("nan")
def pq(xs, q):
    xs = sorted(xs); return xs[min(len(xs) - 1, int(q * len(xs)))] if xs else float("nan")
print(f"# {label}: covered from {on_s:.2f} s to {off_s:.2f} s after the window appeared; 1 s left out at each edge")
print(f"{'phase':<16}{'frames/s':>10}{'int p50 ms':>12}{'int p99 ms':>12}{'int min ms':>12}{'CPU %':>8}")
for name, a, b in (("visible", 2.0, on_s - 1), ("covered", on_s + 1, off_s - 1), ("visible again", off_s + 1, 29.0)):
    ph = [r for r in rows if a <= r[0] <= b]; iv = [r[1] for r in ph if r[1] > 0]
    print(f"{name:<16}{len(ph) / (b - a):>10.1f}{pq(iv, .5):>12.3f}{pq(iv, .99):>12.3f}{min(iv) if iv else float('nan'):>12.3f}{cpu_pct(a, b):>8.1f}")
around = [r for r in rows if off_s - 0.5 <= r[0] <= off_s + 1.5]
gaps = [b[0] - a[0] for a, b in zip(around, around[1:])]
print(f"uncover: frames {off_s - 0.5:.2f}..{off_s + 1.5:.2f} s: {len(around)}, longest gap between frame starts {1000 * max(gaps):.1f} ms" if gaps else "uncover: no frames")
