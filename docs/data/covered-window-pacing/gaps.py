"""gaps.py <out-dir> <label>...: frame gaps over 12 ms (a missed refresh at 120 Hz) and frames under 2 ms apart (and the
longest run of them) while visible, 1 s left out at each edge of the cover."""
import csv, sys
out = sys.argv[1]
for label in sys.argv[2:]:
    ev = dict(l.split() for l in open(f"{out}/{label}-cover.txt"))
    on, off = ((int(ev[k]) - int(ev["seen"])) / 1000 for k in ("on", "off"))
    t = [float(r["t_ms"]) / 1000 for r in csv.DictReader(open(f"{out}/{label}-frames.csv"))]
    for name, a, b in (("visible", 2, on - 1), ("visible again", off + 1, 29)):
        ts = [x for x in t if a <= x <= b]
        g = [(y - x) * 1000 for x, y in zip(ts, ts[1:])]
        run = best = 0
        for x in g:
            run = run + 1 if x < 2 else 0
            best = max(best, run)
        print(f"{label:<8} {name:<14} frames {len(ts):>5}  gaps>12ms {sum(x > 12 for x in g):>3}  gaps<2ms {sum(x < 2 for x in g):>2}  longest run <2ms {best}")
