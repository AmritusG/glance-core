# v0.3.2 diagnosis probes (kept as evidence; none is built by the workspace)

- `safe-drag.swift`, `safe-drop.sh`, `dismiss-crash.sh`: the drop runs from 16:10 on. The drag drops only when the
  topmost window under the drop point is the app under test's, else it carries the drag home and aborts; the run
  stops at the first problem and presses Ignore only on "<test app> quit unexpectedly." dialogs.
- `drop-env.sh`: **unsafe, do not use** — the first runs' harness (a copy of glance-rs `tools/g2-drop-crash.sh`'s
  sample side). It dropped wherever the pointer was; drops landed on other apps and on crash dialogs.
- `alert-key.sh`, `help-check.sh`: one key / one Accessibility press, sent only after checking the test process is
  frontmost and shows the expected window.
- `drop_probe.rs.txt` (run_shell, empty app), `drop_probe2.rs.txt` (winit + wgpu surface; IDLE / IMMEDIATE knobs),
  `alert_probe.rs.txt` (glance_platform::alert inside winit's loop), `winit-min-main.rs.txt` (plain winit 0.30.13,
  POLL / CHURN / LO / HI knobs): temporary examples, deleted from the tree.
- `winit-nsdragoperation.patch.txt`, `winit-fileurl.patch.txt`: the two winit 0.30.13 variants measured (diff -ru
  against the registry source).
