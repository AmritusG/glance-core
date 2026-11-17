# C3 look captures

Window captures (`screencapture -l`, title bar included, 2x) of the same clip at the default size,
for Amrit's side-by-side look review in Glance G2. Clip: `DXT5-DXV3.mov` (DXV3 Normal Quality With
Alpha, 1920x1080, 30 fps) from Amrit's test folder, playing (the frames differ; the chrome is what
is compared). Background black in both. The two PNG captures are not in the public history (they
show a frame of that clip); `stats.txt`, measured from them, is.

- `swift-glance_DXT5-DXV3_default.png` — Swift Glance a9dc601 built from source, bundled under the
  throw-away id `com.amritus.glancecore.swiftref` (its real domain and the installed Glance untouched),
  911 x 616 pt.
- `glance-sample_DXT5-DXV3_default.png` — glance-sample 0.3.0, the notarised build installed in
  /Applications, 960 x 648 pt.
- `stats.txt` — bands from the bottom of each window (`tools/c3-look-stats.py`); read in
  `docs/06-C3-RESULTS.md` §4.
