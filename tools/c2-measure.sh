#!/bin/zsh
# C2 measurement matrix: one clip per window run, 15 s measured after 1 s warm-up.
# c2-measure.sh <folder with synth/> <out> [paces, default "1 2"] [display, as glance-proof --display]
# Each run also logs one CSV row per refresh (c2-<run>-p<pace>.csv).
T=${GLANCE_TESTFILES:?set GLANCE_TESTFILES to the test media folder}
# A clip named neutrally in tests/data/testfiles-by-content.tsv (its own name is not ours to publish),
# found under $T by its size (unique in the folder; the tests also check the hash of its first 64 KiB).
by_content() { find "$T" -type f -size "$(awk -F'\t' -v n="$1" '$1 == n { print $2 }' tests/data/testfiles-by-content.tsv)c" | head -1; }
FILM=$(by_content hap-testclips/std_film_h264_audio.mov)
S=$1; OUT=$2; PACES=(${=${3:-1 2}}); BIN=./target/release/glance-proof
DISPLAY_ARGS=(); [[ -n $4 ]] && DISPLAY_ARGS=(--display "$4")
mkdir -p $OUT
run() { # name pace extra... clip
  local name=$1 pace=$2; shift 2
  pgrep -x glance-proof >/dev/null && pkill -9 -x glance-proof
  $BIN "$@" --seconds 15 --warmup 1 --pace $pace $DISPLAY_ARGS --out $OUT/c2-$name-p$pace.json --log $OUT/c2-$name-p$pace.csv >/dev/null 2>>$OUT/c2-matrix.err
  echo "$name p$pace exit $?"
}
for pace in $PACES; do
  run hap1080-hapq-noaudio $pace "$T/Metric_01_HapM.mov"
  run hap1080-hap-noaudio $pace "$T/glenc_hap1_1920x1080_chunks4.mov"
  run hap2160-hap5-noaudio $pace "$T/glenc_hap5_alpha_real_chunks4.mov"
  run dxv1080-dxt5-noaudio $pace "$T/DXT5-DXV3.mov"
  run dxv1080-ycg6-noaudio $pace "$T/YCG6-DXV3.mov"
  run dxv2160-ycg6-noaudio $pace "$S/synth/dxv_ycg6_3840x2160.mov"
  run dxv2160-dxt1-noaudio $pace "$S/synth/dxv_dxt1_3840x2160.mov"
  run hap2160-hapqa-pcm $pace --mute "$T/glenc_hapm_LEAKTEST_4k.mov"
  run hap2160-hapqa-pcm-soundoff $pace --no-audio "$T/glenc_hapm_LEAKTEST_4k.mov"
  run dxv1080-yg10-pcm $pace --mute "$FILM"
  run dxv1080-yg10-pcm-soundoff $pace --no-audio "$FILM"
  run dxv1080-aac $pace --mute "$T/bad-audio-fixtures/good-audio.mov"
  run hap288-aac $pace --mute "$T/hap-testclips/test-hap1-audio.mov"
done
