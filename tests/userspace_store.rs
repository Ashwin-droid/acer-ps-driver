use std::{
    fs,
    os::unix::fs::{symlink, PermissionsExt},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};
#[test]
fn helper_validates_private_user_owned_data_without_hardware_access() {
    let root = std::env::temp_dir().join(format!(
        "aps-state-test-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let dir = root.join("aps");
    fs::create_dir_all(&dir).unwrap();
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
    let prefs = dir.join("preferences");
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_aps-state"))
            .arg("validate")
            .env("XDG_STATE_HOME", &root)
            .output()
            .unwrap()
    };
    let valid = b"APS-USERSPACE-STATE-1\nprofile=eco\nbutton_mode=hotkey\nkeyboard_timeout=off\n";
    fs::write(&prefs, valid).unwrap();
    fs::set_permissions(&prefs, fs::Permissions::from_mode(0o600)).unwrap();
    assert!(run().status.success());
    // Shutdown during an incomplete restore must not capture boot defaults
    // over the last valid preferences, or touch hardware to do so.
    for message in [
        "waiting to restore; saved preferences retained: profile: I/O error",
        "waiting for readable hardware state; saved preferences retained",
        "error: device unavailable",
        "ready; lid closed, keyboard/Turbo restore deferred until lid opens",
    ] {
        fs::write(dir.join("status"), message).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_aps-state"))
            .arg("save-on-stop")
            .env("XDG_STATE_HOME", &root)
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(fs::read(&prefs).unwrap(), valid);
    }
    fs::set_permissions(&prefs, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(!run().status.success());
    fs::set_permissions(&prefs, fs::Permissions::from_mode(0o600)).unwrap();
    for invalid in [
        b"BAD HEADER".to_vec(),
        b"APS-USERSPACE-STATE-1\nprofile=eco;reboot\n".to_vec(),
        b"APS-USERSPACE-STATE-1\nmux_mode=auto\n".to_vec(),
        b"APS-USERSPACE-STATE-1\nboot_effects=on\n".to_vec(),
        vec![b'x'; 4097],
    ] {
        fs::write(&prefs, invalid).unwrap();
        assert!(!run().status.success());
    }
    fs::remove_file(&prefs).unwrap();
    let sentinel = root.join("sentinel");
    fs::write(&sentinel, valid).unwrap();
    symlink(&sentinel, &prefs).unwrap();
    assert!(!run().status.success());
    assert_eq!(fs::read(&sentinel).unwrap(), valid);
    fs::remove_file(&prefs).unwrap();
    assert!(Command::new("mkfifo")
        .arg("-m")
        .arg("600")
        .arg(&prefs)
        .status()
        .unwrap()
        .success());
    assert!(!run().status.success());
    fs::remove_file(&prefs).unwrap();
    for name in ["status", "lock"] {
        let _ = fs::remove_file(dir.join(name));
    }
    fs::remove_file(sentinel).unwrap();
    fs::remove_dir(dir).unwrap();
    fs::remove_dir(root).unwrap();
}
