//! Restricted userspace persistence worker. Hardware access is only through typed sysfs.
#[path = "../control.rs"]
mod control;
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const SYSFS: &str = "/sys/class/misc/acer-phn16s";
fn store() -> Result<PathBuf> {
    let root = match std::env::var_os("XDG_STATE_HOME") {
        Some(path) if PathBuf::from(&path).is_absolute() => PathBuf::from(path),
        _ => PathBuf::from(std::env::var_os("HOME").ok_or("HOME is unavailable")?)
            .join(".local/state"),
    };
    Ok(root.join("aps"))
}
fn uid() -> Result<u32> {
    Ok(fs::metadata("/proc/self")?.uid())
}
const NOFOLLOW: i32 = 0x20000;
const NONBLOCK: i32 = 0x800;
const ORDER: [&str; 11] = [
    "sleep_logo",
    "profile",
    "keyboard_timeout",
    "keyboard.0",
    "keyboard.1",
    "keyboard.2",
    "keyboard.3",
    "logo",
    "button_mode",
    "turbo_led",
    "turbo_led_mode",
];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Snapshot(BTreeMap<String, String>);
fn valid(key: &str, value: &str) -> bool {
    match key {
        "sleep_logo" => control::sleep_logo(value.as_bytes()).is_some(),
        "logo" => control::logo(value.as_bytes()).is_some(),
        "profile" => control::profile(value.as_bytes()).is_some(),
        "button_mode" => control::button_mode(value.as_bytes()).is_some(),
        "keyboard_timeout" => control::timeout(value.as_bytes()).is_some(),
        "turbo_led" => control::turbo_led(value.as_bytes()).is_some(),
        "turbo_led_mode" => control::turbo_led_mode(value.as_bytes()).is_some(),
        "keyboard.0" | "keyboard.1" | "keyboard.2" | "keyboard.3" => {
            control::light(value.as_bytes()).is_some()
        }
        _ => false,
    }
}
impl Snapshot {
    fn parse(text: &str) -> Result<Self> {
        if text.len() > 4096 {
            return Err("preference file exceeds 4096 bytes".into());
        }
        let mut lines = text.lines();
        if lines.next() != Some("APS-USERSPACE-STATE-1") {
            return Err("unsupported preference format".into());
        }
        let mut state = Self::default();
        for line in lines {
            let (key, value) = line.split_once('=').ok_or("malformed preference line")?;
            if !valid(key, value) || state.0.insert(key.into(), value.into()).is_some() {
                return Err(format!("invalid or duplicate preference field: {key}").into());
            }
        }
        Ok(state)
    }
    fn text(&self) -> String {
        let mut text = "APS-USERSPACE-STATE-1\n".to_string();
        for (key, value) in &self.0 {
            text.push_str(&format!("{key}={value}\n"));
        }
        text
    }
}
fn private_metadata(metadata: &fs::Metadata, directory: bool) -> Result<()> {
    if metadata.uid() != uid()?
        || metadata.mode() & 0o077 != 0
        || (directory && !metadata.is_dir())
        || (!directory && (!metadata.is_file() || metadata.nlink() != 1))
    {
        return Err(
            "store must be private, owned by the service user, and regular (no links/devices)"
                .into(),
        );
    }
    Ok(())
}
fn prepare_store() -> Result<()> {
    if uid()? == 0 {
        return Err("Run aps-state as the normal account, not root".into());
    }
    let directory = store()?;
    match fs::symlink_metadata(&directory) {
        Ok(metadata) => private_metadata(&metadata, true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(&directory)?;
            private_metadata(&fs::symlink_metadata(&directory)?, true)
        }
        Err(error) => Err(error.into()),
    }
}
fn read_private(path: &Path) -> Result<Option<Snapshot>> {
    let before = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    private_metadata(&before, false)?;
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(NOFOLLOW | NONBLOCK)
        .open(path)?;
    let after = file.metadata()?;
    private_metadata(&after, false)?;
    if before.ino() != after.ino() || before.dev() != after.dev() {
        return Err("preference file changed during open".into());
    }
    let mut text = String::new();
    file.take(4097).read_to_string(&mut text)?;
    Ok(Some(Snapshot::parse(&text)?))
}
fn load() -> Result<Option<Snapshot>> {
    prepare_store()?;
    read_private(&store()?.join("preferences"))
}
fn save(state: &Snapshot) -> Result<()> {
    // Validate even internally captured data before persisting it.
    Snapshot::parse(&state.text())?;
    prepare_store()?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .custom_flags(NOFOLLOW | NONBLOCK)
        .open(store()?.join("lock"))?;
    private_metadata(&lock.metadata()?, false)?;
    lock.lock()?;
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let temporary = store()?.join(format!(".preferences.{}.{stamp}", std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(NOFOLLOW)
        .open(&temporary)?;
    let result = (|| -> Result<()> {
        file.write_all(state.text().as_bytes())?;
        file.sync_all()?;
        fs::rename(&temporary, store()?.join("preferences"))?;
        File::open(store()?)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}
fn read(name: &str) -> Result<String> {
    Ok(fs::read_to_string(Path::new(SYSFS).join(name))?)
}
fn generation() -> Result<u64> {
    Ok(read("preferences_generation")?.trim().parse()?)
}
fn developer_stamp() -> Result<Option<String>> {
    // A signed development call can change hardware outside normal controls.
    // Never turn an experiment into a boot preference, including on shutdown.
    match fs::read_to_string(Path::new(SYSFS).join("dev_probe")) {
        Ok(text) => {
            let line = text.lines().next().ok_or("empty developer status")?;
            if line
                .split_ascii_whitespace()
                .any(|field| field == "active=1")
            {
                return Err("developer firmware session active; saved preferences retained".into());
            }
            Ok(Some(line.to_string()))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}
fn capture(previous: &Snapshot) -> Result<Option<Snapshot>> {
    let dev_before = developer_stamp()?;
    let before = generation()?;
    let mut state = previous.clone();
    // Configuration-only, available with the lid closed. Missing on an older
    // module during upgrade: retain any previously saved sleep preference.
    merge_sleep_logo_capture(
        &mut state,
        fs::read_to_string(Path::new(SYSFS).join("sleep_logo")),
    )?;
    for name in ["profile", "button_mode"] {
        let value = read(name)?;
        if !valid(name, value.trim()) {
            return Err(format!("invalid sysfs {name}").into());
        }
        state.0.insert(name.into(), value.trim().into());
    }
    let timeout = read("keyboard_timeout")?;
    state.0.insert(
        "keyboard_timeout".into(),
        match timeout.trim() {
            "0" => "off",
            "30" => "on",
            _ => return Err("invalid timeout readback".into()),
        }
        .into(),
    );
    let mode = read("turbo_led_mode")?;
    if mode.trim() != "unknown" {
        if !valid("turbo_led_mode", mode.trim()) {
            return Err("invalid LED mode readback".into());
        }
        state.0.insert("turbo_led_mode".into(), mode.trim().into());
        let color = read("turbo_led_manual")?;
        if !valid("turbo_led", color.trim()) {
            return Err("invalid LED color readback".into());
        }
        state.0.insert("turbo_led".into(), color.trim().into());
    }
    // Optional during upgrade from a module that predates logo controls. An
    // unknown cache is not a claim that the physical logo is off.
    match fs::read_to_string(Path::new(SYSFS).join("logo")) {
        Ok(value) if value.trim() == "unknown" => (),
        Ok(value) => {
            if !valid("logo", value.trim()) {
                return Err("invalid logo state".into());
            }
            state.0.insert("logo".into(), value.trim().into());
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(error) => return Err(error.into()),
    }
    let keyboard = read("keyboard_state")?;
    merge_keyboard_capture(&mut state, &keyboard)?;
    if before != generation()? || dev_before != developer_stamp()? {
        return Ok(None);
    }
    Ok(Some(state))
}

fn merge_sleep_logo_capture(state: &mut Snapshot, value: std::io::Result<String>) -> Result<()> {
    match value {
        Ok(value) if valid("sleep_logo", value.trim()) => {
            state.0.insert("sleep_logo".into(), value.trim().into());
            Ok(())
        }
        Ok(_) => Err("invalid sleep logo state".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn merge_keyboard_capture(state: &mut Snapshot, text: &str) -> Result<()> {
    if text.trim() == "unknown" {
        return Ok(());
    }
    let mut zones = [None; 4];
    // Older records can contain a combined zone mask. Expand in restore order.
    for index in 0..4 {
        if let Some(value) = state.0.get(&format!("keyboard.{index}")) {
            let setting = control::light(value.as_bytes()).ok_or("invalid saved keyboard row")?;
            for (zone, slot) in zones.iter_mut().enumerate() {
                if setting.zones & (1 << zone) != 0 {
                    *slot = Some(setting);
                }
            }
        }
    }
    let mut seen = 0;
    for line in text.lines() {
        let setting = control::light(line.as_bytes()).ok_or("invalid keyboard row")?;
        if !setting.zones.is_power_of_two() || seen & setting.zones != 0 {
            return Err("invalid/duplicate kernel keyboard zone".into());
        }
        seen |= setting.zones;
        zones[setting.zones.trailing_zeros() as usize] = Some(setting);
    }
    if seen == 0 {
        return Err("empty keyboard state".into());
    }
    // A partial restore, interrupted by closing the lid, must not erase unknown
    // zones or renumber zone 3 as zone 1 just because it is the first known row.
    for (zone, setting) in zones.into_iter().enumerate() {
        if let Some(s) = setting {
            state.0.insert(
                format!("keyboard.{zone}"),
                format!(
                    "{} {} {} {} {} {:02X}{:02X}{:02X}",
                    control::effect(s.mode).ok_or("invalid effect")?,
                    s.brightness,
                    s.speed,
                    control::light_parameter(s.mode, s.direction).ok_or("invalid parameter")?,
                    1 << zone,
                    s.rgb[0],
                    s.rgb[1],
                    s.rgb[2]
                ),
            );
        }
    }
    Ok(())
}
fn restore(state: &Snapshot) -> Result<()> {
    let mut pending = state.clone();
    let errors = restore_pending(&mut pending)?;
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; ").into())
    }
}
fn restore_pending(pending: &mut Snapshot) -> Result<Vec<String>> {
    restore_pending_with(pending, |attribute, value| {
        fs::write(Path::new(SYSFS).join(attribute), value)?;
        thread::sleep(Duration::from_millis(25));
        Ok(())
    })
}
fn restore_pending_with(
    pending: &mut Snapshot,
    write: impl FnMut(&str, &str) -> Result<()>,
) -> Result<Vec<String>> {
    restore_pending_with_wait(pending, write, thread::sleep)
}
fn restore_pending_with_wait(
    pending: &mut Snapshot,
    mut write: impl FnMut(&str, &str) -> Result<()>,
    mut wait: impl FnMut(Duration),
) -> Result<Vec<String>> {
    Snapshot::parse(&pending.text())?;
    let mut errors = Vec::new();
    let mut profile_changed = pending.0.contains_key("profile");
    // Fixed order and destinations. The file never supplies paths or executable commands.
    for key in ORDER {
        if matches!(key, "turbo_led" | "turbo_led_mode") {
            // A later successful profile retry would overwrite an earlier LED
            // color. Keep both LED fields pending until profile restore succeeds.
            if pending.0.contains_key("profile") {
                continue;
            }
            if profile_changed && pending.0.contains_key(key) {
                wait(Duration::from_secs(2));
                profile_changed = false;
            }
        }
        // Setting a manual color switches the LED to manual mode. Apply the
        // requested final mode only after the color succeeds, including retries.
        if key == "turbo_led_mode" && pending.0.contains_key("turbo_led") {
            continue;
        }
        if let Some(value) = pending.0.get(key) {
            let attribute = if key.starts_with("keyboard.") {
                "keyboard_effect"
            } else {
                key
            };
            match write(attribute, value) {
                Ok(()) => {
                    pending.0.remove(key);
                }
                Err(error) => errors.push(format!("{key}: {error}")),
            }
        }
    }
    Ok(errors)
}
fn status(message: &str) {
    eprintln!("aps-state: {message}");
    if let Ok(directory) = store() {
        let path = directory.join("status");
        let text = format!(
            "{message}\nstore={}\n",
            directory.join("preferences").display()
        );
        if fs::write(&path, text).is_ok() {
            let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
        }
    }
}

fn current(previous: &Snapshot) -> Result<Snapshot> {
    for _ in 0..30 {
        if let Some(state) = capture(previous)? {
            return Ok(state);
        }
        thread::sleep(Duration::from_millis(100));
    }
    Err("settings keep changing; snapshot postponed".into())
}
fn load_or_discard() -> Snapshot {
    match load() {
        Ok(Some(state)) => state,
        Ok(None) => Snapshot::default(),
        Err(error) => {
            status(&format!(
                "discarded unreadable/invalid preferences: {error}"
            ));
            Snapshot::default()
        }
    }
}
fn restore_and_capture(state: &Snapshot) -> Result<Snapshot> {
    let mut desired = state.clone();
    let mut pending = state.clone();
    let mut last_generation = None;
    let mut absent = false;
    let mut lid_waiting = false;
    let mut logo_settling = LogoSettling::new();
    loop {
        match generation() {
            Ok(next) => {
                if absent || last_generation.is_some_and(|last| next < last) {
                    pending = desired.clone();
                    lid_waiting = false;
                    logo_settling = LogoSettling::new();
                }
                absent = false;
                last_generation = Some(next);
                let errors = restore_pending(&mut pending)?;
                if pending.0.is_empty() {
                    match current(&desired) {
                        Ok(snapshot) => return Ok(snapshot),
                        Err(error) => status(&format!("waiting for readable hardware state; saved preferences retained: {error}")),
                    }
                } else if only_lid_preferences(&pending)
                    && read("lid_state").is_ok_and(|value| value.trim() == "closed")
                {
                    // Booting docked is healthy, not a failed service. Keep
                    // blocked RGB preferences on disk while still saving changes
                    // to profile/logo/buttons. Unknown driver RGB caches must not
                    // replace their saved values. Retry the pending controls on open.
                    match current(&desired) {
                        Ok(mut snapshot) => {
                            // In particular, a deferred Auto setting must survive
                            // a successful manual-color write followed by lid close.
                            for (key, value) in &pending.0 {
                                snapshot.0.insert(key.clone(), value.clone());
                            }
                            if snapshot != desired {
                                save(&snapshot)?;
                                desired = snapshot;
                            }
                            if !lid_waiting {
                                status("ready; lid closed, keyboard/Turbo restore deferred until lid opens");
                                lid_waiting = true;
                            }
                        }
                        Err(error) => status(&format!(
                            "waiting for readable docked state; preferences retained: {error}"
                        )),
                    }
                    if logo_settling.due(logo_settling.started.elapsed()) {
                        if let Err(error) = reapply_current_logo() {
                            eprintln!("aps-state: docked late logo restore failed: {error}");
                        }
                    }
                } else {
                    status(&format!(
                        "waiting to restore; saved preferences retained: {}",
                        errors.join("; ")
                    ));
                }
            }
            Err(error) => {
                absent = true;
                status(&format!(
                    "waiting for APS driver access; saved preferences retained: {error}"
                ));
            }
        }
        thread::sleep(Duration::from_secs(5));
    }
}

fn only_lid_preferences(pending: &Snapshot) -> bool {
    !pending.0.is_empty()
        && pending.0.keys().all(|key| {
            matches!(
                key.as_str(),
                "keyboard.0"
                    | "keyboard.1"
                    | "keyboard.2"
                    | "keyboard.3"
                    | "keyboard_timeout"
                    | "turbo_led"
                    | "turbo_led_mode"
            )
        })
}

// Some boot firmware accepts an early LED write and then resets/overlays it.
// There is no physical logo readback, so make two bounded late submissions,
// not an unending loop that would fight firmware or user experiments.
struct LogoSettling {
    started: Instant,
    next: usize,
}
impl LogoSettling {
    fn new() -> Self {
        Self {
            started: Instant::now(),
            next: 0,
        }
    }
    fn cancel(&mut self) {
        self.next = 2;
    }
    fn due(&mut self, elapsed: Duration) -> bool {
        if let Some(seconds) = [2, 10].get(self.next) {
            if elapsed >= Duration::from_secs(*seconds) {
                self.next += 1;
                return true;
            }
        }
        false
    }
}
fn reapply_current_logo() -> Result<bool> {
    match fs::read_to_string(Path::new(SYSFS).join("logo")) {
        Ok(value) if value.trim() == "unknown" => return Ok(false),
        Ok(value) if !valid("logo", value.trim()) => return Err("invalid cached logo state".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
        _ => (),
    }
    developer_stamp()?;
    // Kernel-side state lookup is atomic with a newer CLI write. No copied
    // logo value crosses the check/write race. This command is never persisted.
    fs::write(Path::new(SYSFS).join("logo"), b"reapply")?;
    Ok(true)
}
fn watch() -> Result<()> {
    prepare_store()?;
    status("waiting to restore; saved preferences retained");
    let mut state = restore_and_capture(&load_or_discard())?;
    save(&state)?;
    status("ready; preferences restored and saved");
    let mut last = generation()?;
    let mut changed = Instant::now();
    let mut saved = Instant::now();
    let mut dirty = false;
    let mut missing = false;
    let mut dev_paused = false;
    let mut logo_settling = LogoSettling::new();
    loop {
        thread::sleep(Duration::from_secs(1));
        if let Err(error) = developer_stamp() {
            logo_settling.cancel();
            if !dev_paused {
                status(&format!("paused preference capture: {error}"));
            }
            dev_paused = true;
            continue;
        }
        if dev_paused {
            status("ready; developer session ended; saved preferences retained");
            dev_paused = false;
        }
        let next = match generation() {
            Ok(value) => value,
            Err(_) => {
                missing = true;
                continue;
            }
        };
        if missing || next < last {
            status("waiting to restore after driver reload; saved preferences retained");
            // Restore the last captured settings, never the new boot defaults.
            state = restore_and_capture(&state)?;
            save(&state)?;
            last = generation()?;
            missing = false;
            logo_settling = LogoSettling::new();
            status("ready; restored after driver reload");
            continue;
        }
        if logo_settling.due(logo_settling.started.elapsed()) {
            match reapply_current_logo() {
                Ok(true) => status("ready; late logo restore submitted using latest user setting"),
                Ok(false) => (),
                Err(error) => status(&format!("ready; late logo restore failed: {error}")),
            }
        }
        if next != last {
            last = next;
            changed = Instant::now();
            dirty = true;
        }
        if dirty
            && (changed.elapsed() >= Duration::from_secs(1)
                || saved.elapsed() >= Duration::from_secs(5))
        {
            match capture(&state) {
                Ok(Some(snapshot)) => {
                    if snapshot != state {
                        match save(&snapshot) {
                            Ok(()) => {
                                state = snapshot;
                                status("ready; preferences saved");
                            }
                            Err(error) => {
                                status(&format!("save failed; will retry: {error}"));
                                saved = Instant::now();
                                continue;
                            }
                        }
                    }
                    dirty = false;
                    saved = Instant::now();
                }
                Ok(None) => (),
                Err(error) => {
                    status(&format!("capture failed; will retry: {error}"));
                    saved = Instant::now();
                }
            }
        }
    }
}
fn run() -> Result<()> {
    match std::env::args().nth(1).as_deref() {
        Some("watch") => watch(),
        Some("save-on-stop")
            if !fs::read_to_string(store()?.join("status")).is_ok_and(|text| {
                text.starts_with("ready;") && !text.starts_with("ready; lid closed,")
            }) =>
        {
            eprintln!(
                "aps-state: restore/capture was not ready; keeping existing saved preferences"
            );
            Ok(())
        }
        Some("save" | "save-on-stop") => {
            let state = current(&load_or_discard())?;
            save(&state)?;
            status("ready; preferences saved");
            Ok(())
        }
        Some("restore") => {
            let state = load()?.ok_or("no saved preferences")?;
            restore(&state)?;
            status("ready; preferences restored");
            Ok(())
        }
        Some("validate") => {
            load()?.ok_or("no saved preferences")?;
            println!("Preference metadata and contents valid");
            Ok(())
        }
        _ => Err(
            "Use aps-state watch|save|save-on-stop|restore|validate (service/admin helper)".into(),
        ),
    }
}
fn main() {
    if uid().ok() == Some(0) {
        eprintln!("aps-state must run as the normal account, not root");
        std::process::exit(1);
    }
    if let Err(error) = run() {
        status(&format!("error: {error}"));
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sleep_and_awake_logo_records_stay_separate() {
        let state = Snapshot::parse(
            "APS-USERSPACE-STATE-1\nlogo=static 100 FF8C00\nsleep_logo=static 25 0000FF\n",
        )
        .unwrap();
        assert_eq!(Snapshot::parse(&state.text()).unwrap(), state);
        let mut pending = state.clone();
        let mut writes = Vec::new();
        restore_pending_with_wait(
            &mut pending,
            |key, value| {
                writes.push((key.to_string(), value.to_string()));
                Ok(())
            },
            |_| panic!("no profile changed"),
        )
        .unwrap();
        assert_eq!(
            writes,
            [
                ("sleep_logo".into(), "static 25 0000FF".into()),
                ("logo".into(), "static 100 FF8C00".into()),
            ]
        );
        assert!(pending.0.is_empty());
        assert_eq!(state.0["logo"], "static 100 FF8C00");
        assert!(!Snapshot::parse("APS-USERSPACE-STATE-1\nlogo=off\n")
            .unwrap()
            .0
            .contains_key("sleep_logo"));
        for value in [
            "opmode 60",
            "reapply",
            "static 101 0000FF",
            "static 25 GG0000",
            "off\nsleep_logo=off",
            "neon 25 0",
            "raw 83",
        ] {
            assert!(
                Snapshot::parse(&format!("APS-USERSPACE-STATE-1\nsleep_logo={value}\n")).is_err()
            );
        }
    }
    #[test]
    fn sleep_logo_capture_handles_upgrade_and_invalid_readback() {
        let mut state = Snapshot::parse(
            "APS-USERSPACE-STATE-1\nlogo=static 100 FF8C00\nsleep_logo=static 25 0000FF\n",
        )
        .unwrap();
        let before = state.clone();
        merge_sleep_logo_capture(&mut state, Err(std::io::ErrorKind::NotFound.into())).unwrap();
        assert_eq!(state, before);
        for value in ["unknown", "opmode 60", "raw"] {
            assert!(merge_sleep_logo_capture(&mut state, Ok(value.into())).is_err());
            assert_eq!(state, before);
        }
        assert!(merge_sleep_logo_capture(
            &mut state,
            Err(std::io::ErrorKind::PermissionDenied.into())
        )
        .is_err());
        merge_sleep_logo_capture(&mut state, Ok("neon 25 3\n".into())).unwrap();
        assert_eq!(state.0["sleep_logo"], "neon 25 3");
        assert_eq!(state.0["logo"], before.0["logo"]);
    }
    #[test]
    fn sleep_logo_restores_even_if_internal_lighting_is_lid_blocked() {
        let mut pending = Snapshot::parse("APS-USERSPACE-STATE-1\nsleep_logo=static 25 0000FF\nkeyboard.0=static 60 0 none 1 FF0000\nturbo_led=FF0000\n").unwrap();
        let mut writes = Vec::new();
        let errors = restore_pending_with_wait(
            &mut pending,
            |key, _| {
                writes.push(key.to_string());
                if key == "sleep_logo" {
                    Ok(())
                } else {
                    Err("lid closed".into())
                }
            },
            |_| panic!("no profile changed"),
        )
        .unwrap();
        assert_eq!(writes, ["sleep_logo", "keyboard_effect", "turbo_led"]);
        assert_eq!(errors.len(), 2);
        assert!(!pending.0.contains_key("sleep_logo"));
        assert!(only_lid_preferences(&pending));
    }
    #[test]
    fn failed_sleep_preference_restore_is_retained_for_retry() {
        let mut pending =
            Snapshot::parse("APS-USERSPACE-STATE-1\nsleep_logo=neon 25 3\nlogo=off\n").unwrap();
        let errors = restore_pending_with_wait(
            &mut pending,
            |key, _| {
                if key == "sleep_logo" {
                    Err("driver not ready".into())
                } else {
                    Ok(())
                }
            },
            |_| {},
        )
        .unwrap();
        assert_eq!(errors.len(), 1);
        assert_eq!(pending.0.len(), 1);
        assert!(!only_lid_preferences(&pending));
        let mut writes = Vec::new();
        restore_pending_with_wait(
            &mut pending,
            |key, value| {
                writes.push((key.to_string(), value.to_string()));
                Ok(())
            },
            |_| {},
        )
        .unwrap();
        assert_eq!(writes, [("sleep_logo".into(), "neon 25 3".into())]);
        assert!(pending.0.is_empty());
    }
    #[test]
    fn docked_deferral_only_accepts_internal_lighting_fields() {
        let mut pending = Snapshot::parse(
            "APS-USERSPACE-STATE-1\nkeyboard.0=static 60 0 none 1 FF0000\nturbo_led_mode=auto\n",
        )
        .unwrap();
        assert!(only_lid_preferences(&pending));
        pending.0.insert("profile".into(), "eco".into());
        assert!(!only_lid_preferences(&pending));
        assert!(!only_lid_preferences(&Snapshot::default()));
    }
    #[test]
    fn partial_keyboard_capture_retains_uncached_saved_zones() {
        let mut state =
            Snapshot::parse("APS-USERSPACE-STATE-1\nkeyboard.0=static 60 0 none 15 FF0000\n")
                .unwrap();
        merge_keyboard_capture(&mut state, "static 80 0 none 4 0000FF\n").unwrap();
        assert_eq!(state.0.len(), 4);
        assert_eq!(state.0["keyboard.2"], "static 80 0 none 4 0000FF");
        assert_eq!(state.0["keyboard.0"], "static 60 0 none 1 FF0000");
        assert_eq!(state.0["keyboard.3"], "static 60 0 none 8 FF0000");
        let before = state.clone();
        merge_keyboard_capture(&mut state, "unknown\n").unwrap();
        assert_eq!(state, before);
        assert!(merge_keyboard_capture(&mut state, "static 60 0 none 15 FF0000").is_err());
    }
    #[test]
    fn logo_settling_is_bounded_and_can_be_cancelled() {
        let mut replay = LogoSettling::new();
        assert!(!replay.due(Duration::from_secs(1)));
        assert!(replay.due(Duration::from_secs(2)));
        assert!(!replay.due(Duration::from_secs(9)));
        assert!(replay.due(Duration::from_secs(10)));
        assert!(!replay.due(Duration::from_secs(100)));
        let mut cancelled = LogoSettling::new();
        cancelled.cancel();
        assert!(!cancelled.due(Duration::from_secs(100)));
        assert!(Snapshot::parse("APS-USERSPACE-STATE-1\nlogo=reapply\n").is_err());
    }
    #[test]
    fn turbo_led_is_last_with_a_two_second_profile_settle() {
        let events = std::cell::RefCell::new(Vec::new());
        let mut pending = Snapshot::parse("APS-USERSPACE-STATE-1\nprofile=eco\nlogo=static 60 FF8C00\nbutton_mode=hotkey\nturbo_led=B3001B\nturbo_led_mode=manual\n").unwrap();
        restore_pending_with_wait(
            &mut pending,
            |key, _| {
                events.borrow_mut().push(key.to_string());
                Ok(())
            },
            |duration| {
                assert_eq!(duration, Duration::from_secs(2));
                events.borrow_mut().push("wait-2s".into());
            },
        )
        .unwrap();
        assert_eq!(
            *events.borrow(),
            [
                "profile",
                "logo",
                "button_mode",
                "wait-2s",
                "turbo_led",
                "turbo_led_mode"
            ]
        );
        assert!(pending.0.is_empty());
    }
    #[test]
    fn failed_profile_keeps_led_pending_and_retry_still_waits() {
        let mut pending = Snapshot::parse("APS-USERSPACE-STATE-1\nprofile=eco\nlogo=off\nturbo_led=B3001B\nturbo_led_mode=manual\n").unwrap();
        let events = std::cell::RefCell::new(Vec::new());
        let errors = restore_pending_with_wait(
            &mut pending,
            |key, _| {
                events.borrow_mut().push(key.to_string());
                if key == "profile" {
                    Err("profile failed".into())
                } else {
                    Ok(())
                }
            },
            |_| panic!("must not wait for an unsuccessful profile restore"),
        )
        .unwrap();
        assert_eq!(errors.len(), 1);
        assert_eq!(*events.borrow(), ["profile", "logo"]);
        assert!(pending.0.contains_key("turbo_led"));
        assert!(pending.0.contains_key("turbo_led_mode"));
        events.borrow_mut().clear();
        restore_pending_with_wait(
            &mut pending,
            |key, _| {
                events.borrow_mut().push(key.to_string());
                Ok(())
            },
            |duration| {
                assert_eq!(duration, Duration::from_secs(2));
                events.borrow_mut().push("wait-2s".into());
            },
        )
        .unwrap();
        assert_eq!(
            *events.borrow(),
            ["profile", "wait-2s", "turbo_led", "turbo_led_mode"]
        );
        assert!(pending.0.is_empty());
    }
    #[test]
    fn record_roundtrip_and_rejects_commands() {
        let data="APS-USERSPACE-STATE-1\nprofile=eco\nbutton_mode=hotkey\nkeyboard_timeout=off\nturbo_led_mode=auto\nturbo_led=33F000\nkeyboard.0=wave 100 9 right 1 FF8000\n";
        let state = Snapshot::parse(data).unwrap();
        assert_eq!(Snapshot::parse(&state.text()).unwrap(), state);
        for invalid in [
            "APS-USERSPACE-STATE-1\nprofile=eco; reboot\n",
            "APS-USERSPACE-STATE-1\n/etc/passwd=x\n",
            "APS-USERSPACE-STATE-1\nprofile=eco\nprofile=quiet\n",
            "APS-USERSPACE-STATE-1\nbutton_mode=hotkey F16\n",
            "APS-USERSPACE-STATE-1\nkeyboard.0=static 999 0 none 15 FF0000\n",
        ] {
            assert!(Snapshot::parse(invalid).is_err());
        }
        assert!(Snapshot::parse(&"x".repeat(4097)).is_err());
    }
    #[test]
    fn logo_preferences_preserve_modes_without_accepting_raw_commands() {
        for value in [
            "off",
            "static 60 FF00FF",
            "breathing 60 3",
            "neon 40 5",
            "opmode 60",
        ] {
            let record =
                Snapshot::parse(&format!("APS-USERSPACE-STATE-1\nlogo={value}\n")).unwrap();
            assert_eq!(Snapshot::parse(&record.text()).unwrap(), record);
            let mut pending = record.clone();
            restore_pending_with(&mut pending, |attribute, text| {
                assert_eq!(attribute, "logo");
                assert_eq!(text, value);
                Ok(())
            })
            .unwrap();
            assert!(pending.0.is_empty());
        }
        assert!(Snapshot::parse("APS-USERSPACE-STATE-1\nlogo=raw a4 83\n").is_err());
        assert!(Snapshot::parse("APS-USERSPACE-STATE-1\nlogo=shifting 60 3\n").is_err());
    }
    #[test]
    fn failed_profile_does_not_block_lighting_and_only_failures_are_retried() {
        let original = Snapshot::parse(
            "APS-USERSPACE-STATE-1\nprofile=eco\nbutton_mode=hotkey\nkeyboard_timeout=off\n",
        )
        .unwrap();
        let mut pending = original.clone();
        let mut calls = Vec::new();
        let errors = restore_pending_with(&mut pending, |key, _| {
            calls.push(key.to_string());
            if key == "profile" {
                Err(std::io::Error::from_raw_os_error(5).into())
            } else {
                Ok(())
            }
        })
        .unwrap();
        assert_eq!(calls, ["profile", "keyboard_timeout", "button_mode"]);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].starts_with("profile:"));
        assert_eq!(pending.0.len(), 1);
        assert_eq!(original.0.len(), 3);
        calls.clear();
        assert!(restore_pending_with(&mut pending, |key, _| {
            calls.push(key.to_string());
            Ok(())
        })
        .unwrap()
        .is_empty());
        assert_eq!(calls, ["profile"]);
        assert!(pending.0.is_empty());
    }
    #[test]
    fn failed_led_color_defers_final_auto_mode_until_retry() {
        let mut pending = Snapshot::parse(
            "APS-USERSPACE-STATE-1\nturbo_led=33F000\nturbo_led_mode=auto\nbutton_mode=hotkey\n",
        )
        .unwrap();
        let mut calls = Vec::new();
        restore_pending_with(&mut pending, |key, _| {
            calls.push(key.to_string());
            if key == "turbo_led" {
                Err("device unavailable".into())
            } else {
                Ok(())
            }
        })
        .unwrap();
        assert_eq!(calls, ["button_mode", "turbo_led"]);
        calls.clear();
        restore_pending_with(&mut pending, |key, _| {
            calls.push(key.to_string());
            Ok(())
        })
        .unwrap();
        assert_eq!(calls, ["turbo_led", "turbo_led_mode"]);
        assert!(pending.0.is_empty());
    }
    #[test]
    fn invalid_restore_is_rejected_before_any_write() {
        let mut pending = Snapshot(BTreeMap::from([("profile".into(), "2".into())]));
        assert!(
            restore_pending_with(&mut pending, |_, _| panic!("invalid record reached writer"))
                .is_err()
        );
    }
    #[test]
    fn breathing_color_choice_survives_preference_roundtrip() {
        for choice in ["cycle", "none"] {
            let text =
                format!("APS-USERSPACE-STATE-1\nkeyboard.0=breathing 60 3 {choice} 1 0088FF\n");
            let original = Snapshot::parse(&text).unwrap();
            let mut pending = Snapshot::parse(&original.text()).unwrap();
            restore_pending_with(&mut pending, |attribute, value| {
                assert_eq!(attribute, "keyboard_effect");
                assert_eq!(value, format!("breathing 60 3 {choice} 1 0088FF"));
                Ok(())
            })
            .unwrap();
            assert!(pending.0.is_empty());
        }
    }
}
