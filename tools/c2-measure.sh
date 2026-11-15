#!/bin/zsh
# C2 measurement matrix: one clip per window run, 15 s measured after 1 s warm-up.
T=<testfiles>
S=$1; OUT=$2; BIN=./target/release/glance-proof
mkdir -p $OUT
run() { # name pace extra... clip
  local name=$1 pace=$2; shift 2
  pgrep -x glance-proof >/dev/null && pkill -9 -x glance-proof
  $BIN "$@" --seconds 15 --warmup 1 --pace $pace --out $OUT/c2-$name-p$pace.json >/dev/null 2>>$OUT/c2-matrix.err
  echo "$name p$pace exit $?"
}
for pace in 1 2; do
  run hap1080-hapq-noaudio $pace "$T/Metric_01_HapM.mov"
  run hap1080-hap-noaudio $pace "$T/glenc_hap1_1920x1080_chunks4.mov"
  run hap2160-hap5-noaudio $pace "$T/glenc_hap5_alpha_real_chunks4.mov"
  run dxv1080-dxt5-noaudio $pace "$T/DXT5-DXV3.mov"
  run dxv1080-ycg6-noaudio $pace "$T/YCG6-DXV3.mov"
  run dxv2160-ycg6-noaudio $pace "$S/synth/dxv_ycg6_3840x2160.mov"
  run dxv2160-dxt1-noaudio $pace "$S/synth/dxv_dxt1_3840x2160.mov"
  run hap2160-hapqa-pcm $pace --mute "$T/glenc_hapm_LEAKTEST_4k.mov"
  run hap2160-hapqa-pcm-soundoff $pace --no-audio "$T/glenc_hapm_LEAKTEST_4k.mov"
  run dxv1080-yg10-pcm $pace --mute "$T/hap-testclips/std_film_h264_audio.mov"
  run dxv1080-yg10-pcm-soundoff $pace --no-audio "$T/hap-testclips/std_film_h264_audio.mov"
  run dxv1080-aac $pace --mute "$T/bad-audio-fixtures/good-audio.mov"
  run hap288-aac $pace --mute "$T/hap-testclips/test-hap1-audio.mov"
done
