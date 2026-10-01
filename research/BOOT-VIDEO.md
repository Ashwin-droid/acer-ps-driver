# Video input and boot-logo limits

Implementation research, 2026-10-01. No actual ESP, boot flag or kernel setting
was changed for these checks. Conversion and test FAT images are userspace only.

## Acer's exact limit

The supplied PredatorSense 5.1.514 application, extracted locally at
`research/acer-app/dist/widgets/main/main.36216cba6144c871.js`, contains
`checkInputImageAvailable(width, height, size)`. It rejects `size > 35e6`, then
checks width/height against its calculated logo limits. The screen-size setup
uses `Math.floor(.4 * width)` and `Math.floor(.4 * height)`.

The adjacent English resource
`research/acer-app/dist/widgets/main/shared_assets/i18n/en.json` labels the
formats as JPEG/GIF, below 35 MB, with color depth below 32 bits. The executable
comparison permits exactly 35,000,000 bytes. APS now uses that exact decimal
boundary in conversion and elevated-helper validation, replacing its earlier
35 MiB-minus-one bound. No vendor application code is shipped with this change.

## Additional APS memory policy

The prior offline V1.26 analysis in `research/boot-gif-v126/REPORT.md` found that
GifDecoderDxe retains four decoded bytes per pixel of each GIF image descriptor
in a linked frame list. This is older firmware-family evidence, not a guarantee
about every allocation in running V1.28. A 600-frame 1024x640 animation could
require about 1.46 GiB for those rectangles despite being small on disk.

APS therefore additionally limits the sum of encoded descriptor areas times
four to 64 MiB. It counts actual rectangles, not full logical canvases, so small
delta-frame animations are not penalized as if every frame were full-screen.
The budget excludes firmware overhead/scratch memory. It is a conservative APS
policy, not a discovered Acer hardware maximum or proof against firmware bugs.
The shared validator checks rectangle bounds before full decoding and enforces
at most 600 frames and 30 seconds. The root writer repeats these checks.

## Conversion contract

- Regular local input up to 100 MiB. JPEG/PNG/GIF use image signatures; MP4/MOV,
  MKV/WebM and AVI use fixed container signatures and explicit FFmpeg demuxers.
- No URL/playlist autodetection, source symlinks or MOV external references.
  FFmpeg reads a private bounded snapshot, not the original path.
- Select the first real video stream, excluding attached cover art. No audio,
  subtitles or data streams in output. Autorotation precedes crop and preview.
- Export the first 30 seconds at up to 20 fps. Try 1024x640 at 20/10/5/2 fps,
  then 768x480 at 1 fps, stopping at the first fully validated result.
- Per-frame palettes avoid buffering the entire animation behind a global
  palette-generation branch. FFmpeg has bounded threads, decoder pixels,
  output bytes, a 90-second per-attempt timeout and a 180-second conversion
  ladder timeout. Preview decoding has its own 30-second timeout.
- `prepare` creates a new local file. `set` and the picker pass only prepared
  JPEG/GIF to the existing Polkit-protected transactional ESP writer. Conversion
  refuses to run as root through the CLI. No kernel or DKMS update is needed.

## Tests

Real FFmpeg tests cover video plus audio, header-based detection without an
extension, rotation, multi-frame output, automatic frame-rate reduction, and
refusal to overwrite output. Negative tests cover playlists, URLs, malformed
containers, audio-only files and symlinks. GIF tests distinguish compressed
size from decoded rectangle cost and retain valid delta-frame GIFs.

The picker test lists and previews a WebM first frame. The disposable FAT test
converts MP4, installs the GIF at `EFI/OEM/AcerLogo.gif`, verifies the byte-for-byte
readback and JPEG backup, then exercises rollback and reset. It never opens a
real block device or calls a firmware setter. Actual firmware playback of a
video-derived GIF still requires a separately authorized install and reboot.

## Installed verification

All 129 host tests passed, along with Clippy with warnings denied, formatting
and diff checks. Release builds of `/usr/local/sbin/aps` and
`/usr/local/libexec/aps-boot` were installed; their SHA256 hashes match the local
release outputs. Prior binaries remain in
`/var/lib/aps/boot-video-update.PlzNZG`. No service or kernel reload was needed.

The installed `aps boot logo prepare` converted a generated MP4 with audio to a
1024x640 GIF, 1,991,815 bytes. This command only wrote a local temporary output.
No actual boot logo was replaced and no firmware playback result is claimed.
