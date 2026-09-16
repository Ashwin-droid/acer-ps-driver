//! File picker and crop preview. Runs entirely as the normal user.
use crate::boot_media::{self, Crop, Result, HEIGHT, WIDTH};
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
        MouseEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Layout, Rect},
    style::{Color, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Terminal,
};
use ratatui_image::{picker::Picker, protocol::StatefulProtocol, StatefulImage};
use std::{
    fs,
    io::{self, IsTerminal},
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
    time::{Duration, Instant},
};

struct Screen;
impl Drop for Screen {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), DisableMouseCapture, LeaveAlternateScreen);
    }
}

struct Thumbnail {
    image: image::DynamicImage,
    dimensions: (u32, u32),
    gif: bool,
}
fn thumbnail(path: &Path) -> Result<Thumbnail> {
    let (bytes, format, dimensions) = boot_media::source(path)?;
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode()?.thumbnail(640, 480);
    Ok(Thumbnail {
        image,
        dimensions,
        gif: format == image::ImageFormat::Gif,
    })
}
#[derive(Default)]
struct PreviewJobs {
    epoch: u64,
    pending: Option<(u64, PathBuf)>,
    ready: Option<std::result::Result<Thumbnail, String>>,
    stopped: bool,
}
impl PreviewJobs {
    fn finish(&mut self, epoch: u64, result: std::result::Result<Thumbnail, String>) {
        if !self.stopped && self.epoch == epoch {
            self.ready = Some(result);
        }
    }
}
// One decoder and one replaceable pending job, not a thread per highlighted file.
// Epoch checks prevent slow old images from replacing the current selection.
struct PreviewLoader {
    shared: Arc<(Mutex<PreviewJobs>, Condvar)>,
}
impl PreviewLoader {
    fn new() -> Self {
        let shared = Arc::new((Mutex::new(PreviewJobs::default()), Condvar::new()));
        let worker = Arc::clone(&shared);
        std::thread::spawn(move || loop {
            let (lock, changed) = &*worker;
            let (epoch, path) = {
                let mut jobs = lock.lock().unwrap();
                while jobs.pending.is_none() && !jobs.stopped {
                    jobs = changed.wait(jobs).unwrap();
                }
                if jobs.stopped {
                    return;
                }
                jobs.pending.take().unwrap()
            };
            let result = thumbnail(&path).map_err(|e| e.to_string());
            let mut jobs = lock.lock().unwrap();
            if jobs.stopped {
                return;
            }
            jobs.finish(epoch, result);
        });
        Self { shared }
    }
    fn clear(&self) {
        let mut jobs = self.shared.0.lock().unwrap();
        jobs.epoch = jobs.epoch.wrapping_add(1);
        jobs.pending = None;
        jobs.ready = None;
    }
    fn queue(&self, path: PathBuf) {
        let mut jobs = self.shared.0.lock().unwrap();
        jobs.pending = Some((jobs.epoch, path));
        self.shared.1.notify_one();
    }
    fn take(&self) -> Option<std::result::Result<Thumbnail, String>> {
        self.shared.0.lock().unwrap().ready.take()
    }
}
impl Drop for PreviewLoader {
    fn drop(&mut self) {
        let mut jobs = self.shared.0.lock().unwrap();
        jobs.stopped = true;
        jobs.pending = None;
        self.shared.1.notify_one();
        // Do not block cancellation waiting for an in-progress image decode.
    }
}
fn browser_areas(area: Rect) -> [Rect; 4] {
    let rows = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(0),
        Constraint::Length(2),
    ])
    .split(area);
    let panes = if area.width >= 70 {
        Layout::horizontal([Constraint::Percentage(45), Constraint::Percentage(55)]).split(rows[1])
    } else {
        Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)]).split(rows[1])
    };
    [rows[0], panes[0], panes[1], rows[2]]
}
fn hover_index(area: Rect, column: u16, row: u16, offset: usize, len: usize) -> Option<usize> {
    let inner = Block::default().borders(Borders::ALL).inner(area);
    if column < inner.x || column >= inner.right() || row < inner.y || row >= inner.bottom() {
        return None;
    }
    let index = offset + usize::from(row - inner.y);
    (index < len).then_some(index)
}
fn clean(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).take(180).collect()
}
fn files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    if let Some(parent) = dir.parent() {
        paths.push(parent.to_owned());
    }
    let mut children = Vec::new();
    for entry in fs::read_dir(dir)?.take(5000) {
        let entry = entry?;
        let kind = entry.file_type()?;
        let path = entry.path();
        if kind.is_dir()
            || (kind.is_file()
                && path.extension().is_some_and(|e| {
                    matches!(
                        e.to_string_lossy().to_ascii_lowercase().as_str(),
                        "jpg" | "jpeg" | "png" | "gif"
                    )
                }))
        {
            children.push(path);
        }
    }
    children.sort_by_key(|p| {
        (
            !p.is_dir(),
            p.file_name().unwrap_or_default().to_os_string(),
        )
    });
    paths.extend(children);
    Ok(paths)
}

pub(crate) fn pick(start: &Path) -> Result<Option<(tempfile::TempDir, PathBuf)>> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(
            "The image picker needs a terminal. Use aps boot logo set FILE for automatic cropping."
                .into(),
        );
    }
    let mut dir = fs::canonicalize(start)?;
    if !dir.is_dir() {
        return Err("Picker expects a directory".into());
    }
    enable_raw_mode()?;
    let guard = Screen;
    execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    let picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::from_fontsize((8, 16)));
    let mut rows = files(&dir)?;
    let mut selected = ListState::default();
    selected.select(Some(0));
    let loader = PreviewLoader::new();
    let mut highlighted = None;
    let mut pending = None;
    let mut highlight_time = Instant::now();
    let mut browse_image: Option<StatefulProtocol> = None;
    let mut preview_text = String::from("Highlight an image to preview it");
    let mut list_area = Rect::default();
    let chosen = loop {
        let current = selected
            .selected()
            .and_then(|index| rows.get(index))
            .cloned();
        if current != highlighted {
            loader.clear();
            browse_image = None;
            pending = None;
            highlight_time = Instant::now();
            preview_text = match current.as_ref() {
                Some(path) if path.is_dir() => "Folder · Enter to open".into(),
                Some(path) => {
                    pending = Some(path.clone());
                    "Loading preview…".into()
                }
                None => "No images in this folder".into(),
            };
            highlighted = current;
        }
        if highlight_time.elapsed() >= Duration::from_millis(150) {
            if let Some(path) = pending.take() {
                loader.queue(path);
            }
        }
        if let Some(result) = loader.take() {
            match result {
                Ok(thumb) => {
                    preview_text = format!(
                        "{}×{}{}",
                        thumb.dimensions.0,
                        thumb.dimensions.1,
                        if thumb.gif { " · GIF first frame" } else { "" }
                    );
                    browse_image = Some(picker.new_resize_protocol(thumb.image));
                }
                Err(error) => {
                    preview_text = format!(
                        "Preview unavailable: {}\nEnter to try opening the image",
                        clean(&error)
                    )
                }
            }
        }
        terminal.draw(|f| {
            let chunks = browser_areas(f.area());
            list_area = chunks[1];
            f.render_widget(
                Paragraph::new(clean(&dir.display().to_string())).block(
                    Block::default()
                        .title(" Boot logo · choose image ")
                        .borders(Borders::ALL),
                ),
                chunks[0],
            );
            let items: Vec<_> = rows
                .iter()
                .map(|p| {
                    ListItem::new(format!(
                        "{} {}",
                        if p.is_dir() { "▸" } else { " " },
                        if Some(p.as_path()) == dir.parent() {
                            "..".into()
                        } else {
                            clean(&p.file_name().unwrap_or_default().to_string_lossy())
                        }
                    ))
                })
                .collect();
            f.render_stateful_widget(
                List::new(items)
                    .block(Block::default().title(" Files ").borders(Borders::ALL))
                    .highlight_style(Style::default().bg(Color::DarkGray).fg(Color::White)),
                chunks[1],
                &mut selected,
            );
            let preview_block = Block::default().title(" Preview ").borders(Borders::ALL);
            let inner = preview_block.inner(chunks[2]);
            f.render_widget(preview_block, chunks[2]);
            let preview_rows =
                Layout::vertical([Constraint::Length(2), Constraint::Min(0)]).split(inner);
            f.render_widget(Paragraph::new(preview_text.as_str()), preview_rows[0]);
            if let Some(image) = browse_image.as_mut() {
                f.render_stateful_widget(StatefulImage::default(), preview_rows[1], image);
            }
            f.render_widget(
                Paragraph::new(
                    "↑↓ / hover preview   Enter open/crop   Backspace parent   q/Esc cancel",
                ),
                chunks[3],
            );
        })?;
        if !event::poll(Duration::from_millis(100))? {
            continue;
        }
        let event = event::read()?;
        if let Event::Mouse(mouse) = event {
            match mouse.kind {
                MouseEventKind::Moved | MouseEventKind::Down(event::MouseButton::Left) => {
                    if let Some(index) = hover_index(
                        list_area,
                        mouse.column,
                        mouse.row,
                        selected.offset(),
                        rows.len(),
                    ) {
                        selected.select(Some(index));
                    }
                }
                MouseEventKind::ScrollUp => {
                    selected.select(Some(selected.selected().unwrap_or(0).saturating_sub(1)))
                }
                MouseEventKind::ScrollDown => selected.select(Some(
                    (selected.selected().unwrap_or(0) + 1).min(rows.len().saturating_sub(1)),
                )),
                _ => {}
            }
            continue;
        }
        let Event::Key(key) = event else {
            continue;
        };
        if key.kind == KeyEventKind::Release {
            continue;
        }
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return Ok(None);
        }
        let index = selected.selected().unwrap_or(0);
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                drop(guard);
                return Ok(None);
            }
            KeyCode::Up => selected.select(Some(index.saturating_sub(1))),
            KeyCode::Down => selected.select(Some((index + 1).min(rows.len().saturating_sub(1)))),
            KeyCode::Backspace => {
                if let Some(parent) = dir.parent() {
                    dir = parent.to_owned();
                    rows = files(&dir)?;
                    selected.select(Some(0));
                }
            }
            KeyCode::Enter => {
                if let Some(path) = rows.get(index) {
                    if path.is_dir() {
                        dir = path.clone();
                        rows = files(&dir)?;
                        selected.select(Some(0));
                    } else {
                        break path.clone();
                    }
                }
            }
            _ => {}
        }
    };
    drop(loader);
    let work = tempfile::tempdir()?;
    let (bytes, format, dimensions) = boot_media::source(&chosen)?;
    let input = work.path().join("source");
    fs::write(&input, bytes)?;
    let preview = work.path().join("preview.png");
    terminal.draw(|f| f.render_widget(Paragraph::new("Preparing image preview…"), f.area()))?;
    boot_media::run_ffmpeg(
        &[
            "-protocol_whitelist".into(),
            "file,pipe".into(),
            "-i".into(),
            input.clone().into_os_string(),
            "-frames:v".into(),
            "1".into(),
            "-vf".into(),
            "scale=1024:1024:force_original_aspect_ratio=decrease".into(),
            preview.clone().into_os_string(),
        ],
        Duration::from_secs(30),
    )?;
    let original = image::open(preview)?;
    let mut crop = Crop::default();
    let mut image = None;
    let mut changed = true;
    loop {
        if changed {
            let (x, y, w, h) = crop.rect(original.width(), original.height());
            let cropped = original.crop_imm(x, y, w, h).resize_exact(
                512,
                320,
                image::imageops::FilterType::Triangle,
            );
            image = Some(picker.new_resize_protocol(cropped));
            changed = false;
        }
        terminal.draw(|f| {
            let chunks=Layout::vertical([Constraint::Length(3),Constraint::Min(3),Constraint::Length(4)]).split(f.area());
            f.render_widget(Paragraph::new(format!("{} · {}×{} → {WIDTH}×{HEIGHT} · zoom {}%",clean(&chosen.file_name().unwrap_or_default().to_string_lossy()),dimensions.0,dimensions.1,crop.zoom)).block(Block::default().title(" Boot logo · crop preview ").borders(Borders::ALL)),chunks[0]);
            if let Some(state)=image.as_mut() { f.render_stateful_widget(StatefulImage::default(),chunks[1],state); }
            let animation=if format==image::ImageFormat::Gif { "GIF preview is the first frame; export keeps animation, up to 30s at 20fps." } else { "Automatic center crop. Export: JPEG." };
            f.render_widget(Paragraph::new(format!("{animation}\nArrows move crop   +/- zoom   0 reset\nEnter prepare & authorize install   q/Esc cancel")),chunks[2]);
        })?;
        if !event::poll(Duration::from_millis(100))? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind == KeyEventKind::Release {
            continue;
        }
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return Ok(None);
        }
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                drop(guard);
                return Ok(None);
            }
            KeyCode::Left => crop.x = crop.x.saturating_sub(25),
            KeyCode::Right => crop.x = (crop.x + 25).min(1000),
            KeyCode::Up => crop.y = crop.y.saturating_sub(25),
            KeyCode::Down => crop.y = (crop.y + 25).min(1000),
            KeyCode::Char('+') | KeyCode::Char('=') => crop.zoom = (crop.zoom + 10).min(400),
            KeyCode::Char('-') => crop.zoom = crop.zoom.saturating_sub(10).max(100),
            KeyCode::Char('0') => crop = Crop::default(),
            KeyCode::Enter => break,
            _ => continue,
        }
        changed = true;
    }
    drop(terminal);
    drop(guard);
    println!("Preparing {WIDTH}×{HEIGHT} boot logo…");
    let output = work.path().join("prepared");
    boot_media::prepare(&input, &output, crop)?;
    Ok(Some((work, output)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_fits_wide_and_narrow_terminals() {
        let wide = browser_areas(Rect::new(0, 0, 100, 30));
        assert!(wide[1].right() <= wide[2].x);
        assert!(wide[2].width > 0);
        let narrow = browser_areas(Rect::new(0, 0, 60, 30));
        assert!(narrow[1].bottom() <= narrow[2].y);
        assert!(narrow[2].height > 0);
        let _ = browser_areas(Rect::new(0, 0, 20, 4));
    }
    #[test]
    fn hover_respects_borders_scroll_offset_and_row_count() {
        let area = Rect::new(0, 3, 45, 20);
        assert_eq!(hover_index(area, 4, 5, 7, 50), Some(8));
        assert_eq!(hover_index(area, 0, 5, 7, 50), None);
        assert_eq!(hover_index(area, 4, 3, 7, 50), None);
        assert_eq!(hover_index(area, 50, 5, 7, 50), None);
        assert_eq!(hover_index(area, 4, 5, 7, 8), None);
    }
    #[test]
    fn stale_decodes_cannot_replace_new_selection() {
        let mut jobs = PreviewJobs {
            epoch: 9,
            ..Default::default()
        };
        jobs.finish(8, Err("old image".into()));
        assert!(jobs.ready.is_none());
        jobs.finish(9, Err("new image".into()));
        assert!(jobs.ready.is_some());
        jobs.ready = None;
        jobs.stopped = true;
        jobs.finish(9, Err("after cancellation".into()));
        assert!(jobs.ready.is_none());
    }
    #[test]
    fn loader_previews_images_and_reports_bad_files_without_affecting_browser() {
        let dir = tempfile::tempdir().unwrap();
        let good = dir.path().join("good.png");
        image::RgbImage::new(800, 600).save(&good).unwrap();
        let bad = dir.path().join("bad.png");
        fs::write(&bad, b"not an image").unwrap();
        let loader = PreviewLoader::new();
        for (path, valid) in [(bad, false), (good, true)] {
            loader.clear();
            loader.queue(path);
            let started = Instant::now();
            let result = loop {
                if let Some(result) = loader.take() {
                    break result;
                }
                assert!(started.elapsed() < Duration::from_secs(5));
                std::thread::sleep(Duration::from_millis(10));
            };
            assert_eq!(result.is_ok(), valid);
            if let Ok(thumb) = result {
                assert_eq!(thumb.dimensions, (800, 600));
                assert!(thumb.image.width() <= 640 && thumb.image.height() <= 480);
            }
        }
    }
}
