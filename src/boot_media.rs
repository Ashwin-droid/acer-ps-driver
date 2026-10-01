//! Unprivileged conversion and bounded validation of firmware logo images.
#![allow(dead_code)]
use image::{AnimationDecoder, ImageDecoder};
use std::{
    fs,
    io::{BufReader, Cursor, Read},
    os::unix::fs::OpenOptionsExt,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
pub(crate) type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub(crate) const WIDTH: u32 = 1024;
pub(crate) const HEIGHT: u32 = 640;
// PS 5.1.514 checkInputImageAvailable rejects size > 35e6 (decimal bytes).
pub(crate) const MAX_BYTES: usize = 35_000_000;
const MAX_SOURCE_BYTES: usize = 100 * 1024 * 1024;
// APS policy, not an Acer-advertised limit. The inspected firmware retains
// width * height * 4 bytes for each GIF image descriptor until playback ends.
pub(crate) const MAX_FRAME_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MediaFormat {
    Image(image::ImageFormat),
    Video(&'static str),
}
impl MediaFormat {
    pub(crate) fn animated(self) -> bool {
        matches!(self, Self::Video(_) | Self::Image(image::ImageFormat::Gif))
    }
    fn input_args(self, path: &Path) -> Vec<std::ffi::OsString> {
        let demuxer = match self {
            Self::Image(image::ImageFormat::Jpeg) => "jpeg_pipe",
            Self::Image(image::ImageFormat::Png) => "png_pipe",
            Self::Image(image::ImageFormat::Gif) => "gif",
            Self::Video(format) => format,
            _ => unreachable!("only supported image formats are constructed"),
        };
        let mut args = vec![
            "-protocol_whitelist".into(),
            "file".into(),
            "-f".into(),
            demuxer.into(),
            "-max_pixels".into(),
            "64000000".into(),
        ];
        if demuxer == "mov" {
            // MOV supports references to other local files. Never enable them.
            args.extend([
                "-enable_drefs".into(),
                "0".into(),
                "-use_absolute_path".into(),
                "0".into(),
            ]);
        }
        args.extend(["-i".into(), path.as_os_str().to_owned()]);
        args
    }
}

fn video_format(bytes: &[u8]) -> Option<MediaFormat> {
    // Content signatures, never the filename. Do not autodetect playlists,
    // concat scripts, network URLs, or image-sequence input.
    if bytes.starts_with(&[0x1a, 0x45, 0xdf, 0xa3]) {
        Some(MediaFormat::Video("matroska"))
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"AVI ") {
        Some(MediaFormat::Video("avi"))
    } else if matches!(
        bytes.get(4..8),
        Some(b"ftyp" | b"moov" | b"mdat" | b"wide" | b"free")
    ) {
        Some(MediaFormat::Video("mov"))
    } else {
        None
    }
}

pub(crate) fn supported_extension(path: &Path) -> bool {
    path.extension().is_some_and(|e| {
        matches!(
            e.to_string_lossy().to_ascii_lowercase().as_str(),
            "jpg" | "jpeg" | "png" | "gif" | "mp4" | "m4v" | "mov" | "mkv" | "webm" | "avi"
        )
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Crop {
    pub x: u32,
    pub y: u32,
    pub zoom: u32,
}
impl Default for Crop {
    fn default() -> Self {
        Self {
            x: 500,
            y: 500,
            zoom: 100,
        }
    }
}
impl Crop {
    pub(crate) fn rect(self, width: u32, height: u32) -> (u32, u32, u32, u32) {
        let (w, h) = if u64::from(width) * u64::from(HEIGHT) > u64::from(height) * u64::from(WIDTH)
        {
            (height.saturating_mul(WIDTH) / HEIGHT, height)
        } else {
            (width, width.saturating_mul(HEIGHT) / WIDTH)
        };
        let w = (w.saturating_mul(100) / self.zoom.clamp(100, 400)).clamp(1, width.max(1));
        let h = (h.saturating_mul(100) / self.zoom.clamp(100, 400)).clamp(1, height.max(1));
        (
            (width.saturating_sub(w)) * self.x.min(1000) / 1000,
            (height.saturating_sub(h)) * self.y.min(1000) / 1000,
            w,
            h,
        )
    }
}

pub(crate) fn run_ffmpeg(args: &[std::ffi::OsString], timeout: Duration) -> Result<()> {
    let log = tempfile::tempfile()?;
    let mut child = Command::new("/usr/bin/ffmpeg")
        .args([
            "-nostdin",
            "-hide_banner",
            "-loglevel",
            "error",
            "-threads",
            "2",
            "-filter_threads",
            "1",
            "-filter_complex_threads",
            "1",
            "-n",
        ])
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log.try_clone()?)
        .spawn()?;
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if start.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err("FFmpeg timed out".into());
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    if !status.success() {
        use std::io::{Seek, SeekFrom};
        let mut log = log;
        log.seek(SeekFrom::Start(0))?;
        let mut text = String::new();
        log.take(4096).read_to_string(&mut text)?;
        return Err(format!("FFmpeg failed: {}", text.trim()).into());
    }
    Ok(())
}

pub(crate) fn source(path: &Path) -> Result<(Vec<u8>, MediaFormat, (u32, u32))> {
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    let meta = file.metadata()?;
    if !meta.is_file() || meta.len() > MAX_SOURCE_BYTES as u64 {
        return Err("Choose a regular image/video file no larger than 100 MiB".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_SOURCE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_SOURCE_BYTES {
        return Err("Media grew past the size limit".into());
    }
    let (format, size) = match image::guess_format(&bytes) {
        Ok(
            format @ (image::ImageFormat::Jpeg | image::ImageFormat::Png | image::ImageFormat::Gif),
        ) => (
            MediaFormat::Image(format),
            image::ImageReader::with_format(Cursor::new(&bytes), format).into_dimensions()?,
        ),
        _ => {
            let format =
                video_format(&bytes).ok_or("Choose JPEG, PNG, GIF, MP4/MOV, MKV/WebM or AVI")?;
            let frame = preview(&bytes, format)?;
            (format, (frame.width(), frame.height()))
        }
    };
    if size.0 == 0
        || size.1 == 0
        || size.0 > 16384
        || size.1 > 16384
        || u64::from(size.0) * u64::from(size.1) > 64_000_000
    {
        return Err("Image dimensions exceed the input limit".into());
    }
    Ok((bytes, format, size))
}

// Video previews follow FFmpeg's autorotation, just like final conversion.
// Only the first real video stream is selected; attached cover art is excluded.
pub(crate) fn preview(bytes: &[u8], format: MediaFormat) -> Result<image::DynamicImage> {
    let converted;
    let (bytes, format) = match format {
        MediaFormat::Image(format) => (bytes, format),
        MediaFormat::Video(_) => {
            let work = tempfile::tempdir()?;
            let input = work.path().join("input");
            let output = work.path().join("preview.png");
            fs::write(&input, bytes)?;
            let mut args = format.input_args(&input);
            args.extend([
                "-map".into(),
                "0:V:0".into(),
                "-an".into(),
                "-frames:v".into(),
                "1".into(),
                "-fs".into(),
                MAX_SOURCE_BYTES.to_string().into(),
                output.as_os_str().to_owned(),
            ]);
            run_ffmpeg(&args, Duration::from_secs(30))?;
            converted = read_bounded(&output, MAX_SOURCE_BYTES)?;
            (converted.as_slice(), image::ImageFormat::Png)
        }
    };
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    Ok(reader.decode()?)
}

fn read_bounded(path: &Path, maximum: usize) -> Result<Vec<u8>> {
    let file = fs::File::open(path)?;
    if file.metadata()?.len() > maximum as u64 {
        return Err("Converted media exceeds its byte budget".into());
    }
    let mut bytes = Vec::new();
    file.take(maximum as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > maximum {
        return Err("Converted media exceeds its byte budget".into());
    }
    Ok(bytes)
}

pub(crate) fn prepare(path: &Path, output: &Path, crop: Crop) -> Result<()> {
    // Copy a bounded snapshot first: FFmpeg never follows the original path,
    // network URL, image sequence or a source changed during Polkit authorization.
    let (bytes, format, (width, height)) = source(path)?;
    let work = tempfile::tempdir()?;
    let input = work.path().join("input");
    fs::write(&input, bytes)?;
    let gif = format.animated();
    let (x, y, w, h) = crop.rect(width, height);
    // Bounded quality ladder. Even the last 30s/1fps candidate fits the decoded
    // budget with full-frame descriptors, rather than depending on compression.
    let candidates: &[(u32, u32, u32)] = if gif {
        &[
            (1024, 640, 20),
            (1024, 640, 10),
            (1024, 640, 5),
            (1024, 640, 2),
            (768, 480, 1),
        ]
    } else {
        &[(WIDTH, HEIGHT, 1)]
    };
    let mut prepared = None;
    let mut last_error = String::new();
    let started = Instant::now();
    for &(out_w, out_h, fps) in candidates {
        let converted = work.path().join(format!(
            "logo-{out_w}-{fps}.{}",
            if gif { "gif" } else { "jpg" }
        ));
        let filters = format!("crop={w}:{h}:{x}:{y},scale={out_w}:{out_h}:flags=lanczos,setsar=1");
        let mut args = format.input_args(&input);
        args.extend([
            "-map_metadata".into(),
            "-1".into(),
            "-an".into(),
            "-sn".into(),
            "-dn".into(),
        ]);
        if gif {
            // Single-frame palettes avoid queuing the entire decoded video on
            // the split branch while palettegen waits for EOF.
            args.extend(["-t".into(), "30".into(), "-filter_complex".into(),
                format!("[0:V:0]{filters},fps={fps},split[a][b];[a]palettegen=stats_mode=single:max_colors=256:reserve_transparent=0[p];[b][p]paletteuse=new=1:dither=bayer[out]").into(),
                "-map".into(), "[out]".into(), "-loop".into(), "0".into()]);
        } else {
            args.extend([
                "-map".into(),
                "0:V:0".into(),
                "-vf".into(),
                filters.into(),
                "-frames:v".into(),
                "1".into(),
                "-pix_fmt".into(),
                "yuvj420p".into(),
                "-q:v".into(),
                "2".into(),
            ]);
        }
        args.extend([
            "-fs".into(),
            (MAX_BYTES + 1).to_string().into(),
            converted.as_os_str().to_owned(),
        ]);
        let remaining = Duration::from_secs(180).saturating_sub(started.elapsed());
        if remaining.is_zero() {
            return Err("Boot-logo conversion timed out".into());
        }
        run_ffmpeg(&args, remaining.min(Duration::from_secs(90)))?;
        let result = read_bounded(&converted, MAX_BYTES).and_then(|data| {
            validate(&data)?;
            Ok(data)
        });
        match result {
            Ok(data) => {
                if gif {
                    eprintln!("Boot GIF prepared: {out_w}x{out_h}, up to {fps} fps, {} bytes; first 30s at most, audio removed.", data.len());
                }
                prepared = Some(data);
                break;
            }
            Err(error) => {
                last_error = error.to_string();
                if gif {
                    eprintln!("Retrying boot GIF at lower quality: {last_error}");
                }
            }
        }
    }
    let data =
        prepared.ok_or_else(|| format!("Cannot make a compliant boot logo: {last_error}"))?;
    use std::io::Write;
    let mut dest = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(output)?;
    dest.write_all(&data)?;
    dest.sync_all()?;
    Ok(())
}

pub(crate) fn validate(bytes: &[u8]) -> Result<&'static str> {
    if bytes.is_empty() || bytes.len() > MAX_BYTES {
        return Err(
            "Boot logo must be no larger than 35,000,000 bytes (Acer's 35 MB limit)".into(),
        );
    }
    let format = image::guess_format(bytes)?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(WIDTH);
    limits.max_image_height = Some(HEIGHT);
    limits.max_alloc = Some(64 * 1024 * 1024);
    let size = image::ImageReader::with_format(Cursor::new(bytes), format).into_dimensions()?;
    if size.0 == 0 || size.1 == 0 || size.0 > WIDTH || size.1 > HEIGHT {
        return Err("Boot logo exceeds 1024x640".into());
    }
    match format {
        image::ImageFormat::Jpeg => {
            if !bytes.ends_with(&[0xff, 0xd9]) {
                return Err("JPEG must end at its EOI marker".into());
            }
            let mut reader = image::ImageReader::with_format(Cursor::new(bytes), format);
            reader.limits(limits);
            reader.decode()?;
            Ok("jpg")
        }
        image::ImageFormat::Gif => {
            if bytes.last() != Some(&0x3b) {
                return Err("GIF must end at its trailer".into());
            }
            gif_frame_budget(bytes)?;
            let mut decoder =
                image::codecs::gif::GifDecoder::new(BufReader::new(Cursor::new(bytes)))?;
            decoder.set_limits(limits)?;
            let mut count = 0;
            let mut duration = Duration::ZERO;
            for frame in decoder.into_frames() {
                count += 1;
                if count > 600 {
                    return Err("Boot GIF exceeds 600 frames".into());
                }
                duration += Duration::from(frame?.delay());
                if duration > Duration::from_secs(30) {
                    return Err("Boot GIF exceeds 30 seconds".into());
                }
            }
            if count == 0 {
                return Err("GIF contains no frames".into());
            }
            Ok("gif")
        }
        _ => Err("Firmware accepts only JPEG or GIF; prepare the image first".into()),
    }
}

fn gif_frame_budget(bytes: &[u8]) -> Result<()> {
    let mut options = gif::DecodeOptions::new();
    options.check_frame_consistency(true);
    options.skip_frame_decoding(true);
    let mut decoder = options.read_info(Cursor::new(bytes))?;
    let mut total = 0u64;
    let mut count = 0usize;
    while let Some(frame) = decoder.read_next_frame()? {
        count += 1;
        if count > 600 {
            return Err("Boot GIF exceeds 600 frames".into());
        }
        total = total
            .checked_add(u64::from(frame.width) * u64::from(frame.height) * 4)
            .ok_or("Boot GIF frame budget overflow")?;
        if total > MAX_FRAME_BYTES {
            return Err("Boot GIF exceeds APS's 64 MiB decoded-frame budget".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn video(path: &Path, seconds: &str) {
        run_ffmpeg(
            &[
                "-f".into(),
                "lavfi".into(),
                "-i".into(),
                "testsrc2=size=80x48:rate=20".into(),
                "-f".into(),
                "lavfi".into(),
                "-i".into(),
                "sine=frequency=440:sample_rate=8000".into(),
                "-t".into(),
                seconds.into(),
                "-c:v".into(),
                "mpeg4".into(),
                "-c:a".into(),
                "aac".into(),
                path.as_os_str().to_owned(),
            ],
            Duration::from_secs(15),
        )
        .unwrap();
    }
    fn frames(bytes: &[u8]) -> Vec<image::Frame> {
        image::codecs::gif::GifDecoder::new(BufReader::new(Cursor::new(bytes)))
            .unwrap()
            .into_frames()
            .collect_frames()
            .unwrap()
    }
    #[test]
    fn video_with_audio_becomes_animated_gif_and_cannot_overwrite_output() {
        let dir = tempfile::tempdir().unwrap();
        // No extension: acceptance depends on the container signature.
        let original = dir.path().join("clip.mp4");
        video(&original, "0.4");
        let input = dir.path().join("snapshot");
        fs::rename(original, &input).unwrap();
        let output = dir.path().join("logo.gif");
        let (_, format, size) = source(&input).unwrap();
        assert_eq!(format, MediaFormat::Video("mov"));
        assert_eq!(size, (80, 48));
        prepare(&input, &output, Crop::default()).unwrap();
        let bytes = fs::read(&output).unwrap();
        assert_eq!(validate(&bytes).unwrap(), "gif");
        let decoded = frames(&bytes);
        assert!(decoded.len() > 1 && decoded.len() <= 8);
        assert_eq!(decoded[0].buffer().dimensions(), (WIDTH, HEIGHT));
        assert_ne!(
            decoded.first().unwrap().buffer(),
            decoded.last().unwrap().buffer()
        );
        assert!(prepare(&input, &output, Crop::default()).is_err());
        assert_eq!(fs::read(output).unwrap(), bytes);
        let link = dir.path().join("symlink.mp4");
        std::os::unix::fs::symlink(input, &link).unwrap();
        assert!(source(&link).is_err());
    }
    #[test]
    fn video_reduces_frame_rate_to_fit_decoded_budget() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("clip.mp4");
        video(&input, "3");
        let output = dir.path().join("logo.gif");
        prepare(&input, &output, Crop::default()).unwrap();
        let bytes = fs::read(output).unwrap();
        assert_eq!(validate(&bytes).unwrap(), "gif");
        let decoded = frames(&bytes);
        // At 20fps these full-changing frames would exceed 64MiB decoded.
        assert!(decoded.len() > 1 && decoded.len() < 60);
        let duration: Duration = decoded
            .iter()
            .map(|frame| Duration::from(frame.delay()))
            .sum();
        assert!(duration >= Duration::from_millis(2800) && duration <= Duration::from_secs(3));
    }
    #[test]
    fn rotated_video_preview_and_conversion_agree() {
        let dir = tempfile::tempdir().unwrap();
        let original = dir.path().join("original.mp4");
        let rotated = dir.path().join("rotated.mp4");
        video(&original, "0.2");
        run_ffmpeg(
            &[
                "-display_rotation".into(),
                "90".into(),
                "-i".into(),
                original.into_os_string(),
                "-c".into(),
                "copy".into(),
                rotated.as_os_str().to_owned(),
            ],
            Duration::from_secs(15),
        )
        .unwrap();
        let (bytes, format, size) = source(&rotated).unwrap();
        let preview = preview(&bytes, format).unwrap();
        assert_eq!(size, (48, 80));
        assert_eq!((preview.width(), preview.height()), size);
        let output = dir.path().join("logo.gif");
        prepare(
            &rotated,
            &output,
            Crop {
                x: 1000,
                y: 0,
                zoom: 200,
            },
        )
        .unwrap();
        assert_eq!(validate(&fs::read(output).unwrap()).unwrap(), "gif");
    }
    #[test]
    fn rejects_playlists_audio_and_false_container_headers() {
        let dir = tempfile::tempdir().unwrap();
        for bytes in [
            b"#EXTM3U\nhttps://example.invalid/video.mp4".as_slice(),
            b"ffconcat version 1.0\nfile /etc/passwd",
            b"https://example.invalid/clip.mp4",
            b"\0\0\0\x18ftypbad",
        ] {
            let path = dir.path().join("not-video.mp4");
            fs::write(&path, bytes).unwrap();
            assert!(source(&path).is_err());
        }
        let audio = dir.path().join("audio.mp4");
        run_ffmpeg(
            &[
                "-f".into(),
                "lavfi".into(),
                "-i".into(),
                "sine=frequency=440".into(),
                "-t".into(),
                "0.1".into(),
                "-c:a".into(),
                "aac".into(),
                audio.as_os_str().to_owned(),
            ],
            Duration::from_secs(15),
        )
        .unwrap();
        assert!(source(&audio).is_err());
        for ext in ["mp4", "MOV", "m4v", "mkv", "webm", "avi", "png", "gif"] {
            assert!(supported_extension(Path::new(&format!("clip.{ext}"))));
        }
        assert!(!supported_extension(Path::new("clip.m3u8")));
    }
    #[test]
    fn firmware_budget_counts_encoded_rectangles_not_compressed_bytes() {
        let mut bytes = Vec::new();
        {
            let mut encoder =
                gif::Encoder::new(&mut bytes, WIDTH as u16, HEIGHT as u16, &[0, 0, 0]).unwrap();
            for _ in 0..26 {
                let frame = gif::Frame {
                    width: WIDTH as u16,
                    height: HEIGHT as u16,
                    delay: 5,
                    buffer: vec![0; (WIDTH * HEIGHT) as usize].into(),
                    ..Default::default()
                };
                encoder.write_frame(&frame).unwrap();
            }
        }
        assert!(bytes.len() < MAX_BYTES);
        assert!(validate(&bytes)
            .unwrap_err()
            .to_string()
            .contains("decoded-frame budget"));
        assert_eq!(MAX_BYTES, 35_000_000);
        assert!(validate(&vec![0; MAX_BYTES + 1]).is_err());
        let mut delta = Vec::new();
        {
            let mut encoder =
                gif::Encoder::new(&mut delta, WIDTH as u16, HEIGHT as u16, &[0, 0, 0]).unwrap();
            for _ in 0..100 {
                encoder
                    .write_frame(&gif::Frame {
                        width: 1,
                        height: 1,
                        delay: 5,
                        buffer: vec![0].into(),
                        ..Default::default()
                    })
                    .unwrap();
            }
        }
        assert_eq!(validate(&delta).unwrap(), "gif");
    }
    #[test]
    fn gif_duration_and_frame_count_are_independently_bounded() {
        for (count, delay, valid) in [
            (1, 3000, true),
            (1, 3001, false),
            (600, 5, true),
            (601, 1, false),
        ] {
            let mut bytes = Vec::new();
            {
                let mut encoder = gif::Encoder::new(&mut bytes, 1, 1, &[0, 0, 0]).unwrap();
                for _ in 0..count {
                    encoder
                        .write_frame(&gif::Frame {
                            width: 1,
                            height: 1,
                            delay,
                            buffer: vec![0].into(),
                            ..Default::default()
                        })
                        .unwrap();
                }
            }
            assert_eq!(
                validate(&bytes).is_ok(),
                valid,
                "{count} frames, {delay} centiseconds"
            );
        }
    }
    #[test]
    fn input_limits_apply_before_decode_and_mov_references_stay_disabled() {
        let dir = tempfile::tempdir().unwrap();
        let large = dir.path().join("oversized.mp4");
        fs::File::create(&large)
            .unwrap()
            .set_len(MAX_SOURCE_BYTES as u64 + 1)
            .unwrap();
        assert!(source(&large).unwrap_err().to_string().contains("100 MiB"));
        let args = MediaFormat::Video("mov").input_args(Path::new("/private/snapshot"));
        for pair in [
            ["-protocol_whitelist", "file"],
            ["-f", "mov"],
            ["-enable_drefs", "0"],
            ["-use_absolute_path", "0"],
            ["-max_pixels", "64000000"],
        ] {
            assert!(args
                .windows(2)
                .any(|values| values[0] == pair[0] && values[1] == pair[1]));
        }
    }
    #[test]
    fn auto_crop_and_pan_stay_within_image() {
        assert_eq!(Crop::default().rect(2000, 1000), (200, 0, 1600, 1000));
        for (w, h) in [(1, 1), (2000, 1000), (300, 900), (16384, 3900)] {
            for crop in [
                Crop::default(),
                Crop {
                    x: 0,
                    y: 1000,
                    zoom: 400,
                },
                Crop {
                    x: 1000,
                    y: 0,
                    zoom: 100,
                },
            ] {
                let (x, y, cw, ch) = crop.rect(w, h);
                assert!(x + cw <= w && y + ch <= h && cw > 0 && ch > 0);
            }
        }
    }
    #[test]
    fn conversion_and_validation_use_real_ffmpeg() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("input.png");
        let output = dir.path().join("output.jpg");
        image::RgbImage::from_fn(90, 120, |x, y| image::Rgb([x as u8, y as u8, 100]))
            .save(&input)
            .unwrap();
        prepare(&input, &output, Crop::default()).unwrap();
        assert_eq!(validate(&fs::read(&output).unwrap()).unwrap(), "jpg");
        assert!(prepare(&input, &output, Crop::default()).is_err());
        assert!(validate(b"not an image").is_err());
        assert!(validate(&fs::read(input).unwrap()).is_err());
    }
    #[test]
    fn animated_gif_remains_animated_after_crop() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("input.gif");
        let output = dir.path().join("prepared.gif");
        {
            let mut encoder =
                image::codecs::gif::GifEncoder::new(fs::File::create(&input).unwrap());
            for color in [[255, 0, 0, 255], [0, 0, 255, 255]] {
                encoder
                    .encode_frame(image::Frame::from_parts(
                        image::RgbaImage::from_pixel(40, 40, image::Rgba(color)),
                        0,
                        0,
                        image::Delay::from_numer_denom_ms(200, 1),
                    ))
                    .unwrap();
            }
        }
        prepare(&input, &output, Crop::default()).unwrap();
        let bytes = fs::read(output).unwrap();
        assert_eq!(validate(&bytes).unwrap(), "gif");
        let frames = image::codecs::gif::GifDecoder::new(BufReader::new(Cursor::new(bytes)))
            .unwrap()
            .into_frames()
            .collect_frames()
            .unwrap();
        assert!(frames.len() > 1);
        assert_eq!(frames[0].buffer().dimensions(), (WIDTH, HEIGHT));
    }
    #[test]
    fn rejects_links_and_malformed_prepared_images() {
        let dir = tempfile::tempdir().unwrap();
        let original = dir.path().join("original.png");
        image::RgbImage::new(8, 8).save(&original).unwrap();
        let link = dir.path().join("link.png");
        std::os::unix::fs::symlink(&original, &link).unwrap();
        assert!(source(&link).is_err());
        let mut bytes = Vec::new();
        image::codecs::jpeg::JpegEncoder::new(&mut bytes)
            .encode(&[0; 3], 1, 1, image::ExtendedColorType::Rgb8)
            .unwrap();
        assert!(validate(&bytes).is_ok());
        bytes.pop();
        assert!(validate(&bytes).is_err());
        assert!(validate(&vec![0; MAX_BYTES + 1]).is_err());
    }
}
