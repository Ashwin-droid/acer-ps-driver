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
pub(crate) const MAX_BYTES: usize = 35 * 1024 * 1024 - 1;

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

pub(crate) fn source(path: &Path) -> Result<(Vec<u8>, image::ImageFormat, (u32, u32))> {
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    let meta = file.metadata()?;
    if !meta.is_file() || meta.len() > 100 * 1024 * 1024 {
        return Err("Choose a regular image file smaller than 100 MiB".into());
    }
    let mut bytes = Vec::new();
    file.take(100 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 100 * 1024 * 1024 {
        return Err("Image grew past the size limit".into());
    }
    let format = image::guess_format(&bytes)?;
    if !matches!(
        format,
        image::ImageFormat::Jpeg | image::ImageFormat::Png | image::ImageFormat::Gif
    ) {
        return Err("Choose JPEG, PNG or GIF".into());
    }
    let size = image::ImageReader::with_format(Cursor::new(&bytes), format).into_dimensions()?;
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

pub(crate) fn prepare(path: &Path, output: &Path, crop: Crop) -> Result<()> {
    // Copy a bounded snapshot first: FFmpeg never follows the original path,
    // network URL, image sequence or a source changed during Polkit authorization.
    let (bytes, format, (width, height)) = source(path)?;
    let work = tempfile::tempdir()?;
    let input = work.path().join("input");
    fs::write(&input, bytes)?;
    let gif = format == image::ImageFormat::Gif;
    let converted = work.path().join(if gif { "logo.gif" } else { "logo.jpg" });
    let (x, y, w, h) = crop.rect(width, height);
    let filters = format!("crop={w}:{h}:{x}:{y},scale={WIDTH}:{HEIGHT}:flags=lanczos,setsar=1");
    let mut args: Vec<std::ffi::OsString> = vec![
        "-protocol_whitelist".into(),
        "file,pipe".into(),
        "-i".into(),
        input.into_os_string(),
        "-map_metadata".into(),
        "-1".into(),
        "-an".into(),
    ];
    if gif {
        args.extend(["-t".into(), "30".into(), "-filter_complex".into(), format!("{filters},fps=20,split[a][b];[a]palettegen=max_colors=256:reserve_transparent=0[p];[b][p]paletteuse=dither=bayer").into(), "-loop".into(), "0".into()]);
    } else {
        args.extend([
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
    args.push(converted.clone().into_os_string());
    run_ffmpeg(&args, Duration::from_secs(90))?;
    let data = fs::read(converted)?;
    validate(&data)?;
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
        return Err("Boot logo must be smaller than 35 MiB".into());
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
                if duration > Duration::from_secs(31) {
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

#[cfg(test)]
mod tests {
    use super::*;
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
