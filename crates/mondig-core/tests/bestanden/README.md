# Testbestanden

Synthetisch: een sinustoon van 440 Hz, 2 seconden, met (waar het een video is) een
grijs beeld van 16 bij 16 pixels. Geen opnames van mensen. Gemaakt met ffmpeg 8.0.1:

```sh
F="-hide_banner -loglevel error -y"
S="-f lavfi -i sine=frequency=440:sample_rate=48000:duration=2"
V="-f lavfi -i color=c=gray:s=16x16:r=2:d=2"
ffmpeg $F $S -ac 2 -c:a libopus -b:a 32k -map_metadata -1 sinus.ogg
ffmpeg $F $S -ac 1 -ar 16000 -c:a libopus -b:a 16k -application voip -map_metadata -1 sinus.opus
ffmpeg $F $V $S -ac 2 -c:v libvpx-vp9 -b:v 10k -c:a libopus -b:a 32k -shortest -map_metadata -1 sinus.webm
ffmpeg $F $V $S -ac 2 -c:v libx264 -pix_fmt yuv420p -x264-params info=0 -c:a aac -b:a 32k -shortest -map_metadata -1 sinus.mkv
ffmpeg $F $V $S -ac 2 -c:v libx264 -pix_fmt yuv420p -x264-params info=0 -c:a aac -b:a 32k -shortest -map_metadata -1 sinus.mp4
ffmpeg $F $V $S -ac 2 -c:v libx265 -x265-params info=0:log-level=error -tag:v hvc1 -pix_fmt yuv420p -c:a aac -b:a 32k -shortest -map_metadata -1 sinus.mov
ffmpeg $F $V $S -ac 2 -c:v libx265 -x265-params log-level=error -tag:v hvc1 -pix_fmt yuv420p -c:a aac -b:a 32k -shortest -map_metadata -1 hvcc-te-groot.mov
ffmpeg $F $V $S -ac 2 -c:v mpeg4 -c:a libmp3lame -b:a 32k -shortest -map_metadata -1 sinus.avi
```

`hvcc-te-groot.mov` heeft een `hvcC`-blok van meer dan 1 KB (de encoderinformatie van
x265); symphonia weigert dat (ADR 0015). `sinus.avi` kan Mondig niet lezen.
