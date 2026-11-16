import csv, glob, json, os, sys

# sum60.py <data dir> : one line per run (pass1, pass2), then each skip with the presents around it.
root = sys.argv[1]
runs = []
for p in sorted(glob.glob(f"{root}/pass*/c2-*.json")):
    d = json.load(open(p))
    c = d["clips"][0]
    pi = d["present_interval"]
    name = os.path.basename(p)[3:-8]
    rows = list(csv.DictReader(open(p[:-5] + ".csv")))
    ivs = [float(r["present_interval_ms"]) for r in rows if r["present_interval_ms"]]
    late = sum(1 for v in ivs if v > 25.0)
    skips = []
    frames = c["frames"]
    prev = None
    for i, r in enumerate(rows):
        f = r["clip0_frame"]
        if f == "":
            continue
        f = int(f)
        if prev is not None and f != prev and f != (prev + 1) % frames:
            ctx = [rows[j]["present_interval_ms"] for j in range(max(0, i - 3), i + 1)]
            skips.append((rows[i]["t_ms"], prev, f, ctx))
        prev = f
    runs.append((os.path.basename(os.path.dirname(p)), name, d, c, pi, late, skips))

print("pass | run | display | Hz | shown | skipped | presents | interval p50/p95/p99/max ms | >25 ms | picture max diff | audio")
for pas, name, d, c, pi, late, skips in runs:
    pv = c.get("picture_vs_cpu") or {}
    print(f"{pas} | {name} | {d.get('display')} | {d['display_refresh_hz']} | {c['cadence']['shown']} | {c['cadence']['skipped']} | {d['presents']} | "
          f"{pi['p50_ms']:.2f} / {pi['p95_ms']:.2f} / {pi['p99_ms']:.2f} / {pi['max_ms']:.2f} | {late} | {pv.get('max_diff')} | {c['master']} {c['audio_state']}"
          + (f" | error {c['error']}" if c.get("error") else ""))
tot = sum(c["cadence"]["skipped"] for *_, c, _, _, _ in [(r[0], r[1], r[2], r[3], r[4], r[5], r[6]) for r in runs])
print(f"\nruns {len(runs)}, skipped total {sum(r[3]['cadence']['skipped'] for r in runs)}, shown total {sum(r[3]['cadence']['shown'] for r in runs)}")
for pas, name, d, c, pi, late, skips in runs:
    for t, a, b, ctx in skips:
        print(f"skip {pas} {name}: t={t} ms frame {a} -> {b}; present intervals up to it (ms): {', '.join(ctx)}")
