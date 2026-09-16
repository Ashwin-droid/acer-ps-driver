use std::process::Command;

#[test]
fn help_paths_never_apply_a_default_effect() {
    for args in [
        vec!["lighting"],
        vec!["lighting", "apply"],
        vec!["lighting", "--help"],
        vec!["lighting", "apply", "--help"],
        vec!["lighting", "apply", "breathing", "--help"],
        vec!["help", "lighting"],
        vec!["fans", "--help"],
        vec!["keyboard", "--help"],
        vec!["keyboard", "shifting", "--help"],
        vec!["keyboard", "zoom", "--help"],
        vec!["keyboard", "meteor", "--help"],
        vec!["keyboard", "twinkling", "--help"],
        vec!["profile", "--help"],
        vec!["button", "--help"],
        vec!["turbo-led", "--help"],
        vec!["logo"],
        vec!["logo", "--help"],
        vec!["logo", "breathing", "--help"],
        vec!["help", "logo"],
        vec!["gpu"],
        vec!["gpu", "--help"],
        vec!["gpu", "mux", "auto", "--help"],
        vec!["help", "gpu"],
        vec!["boot"],
        vec!["boot", "--help"],
        vec!["boot", "effects", "on", "--help"],
        vec!["boot", "logo", "reset", "--help"],
        vec!["help", "boot"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_aps"))
            .args(&args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("aps "), "{args:?}");
        assert!(
            !stdout.contains("Applied ") && !stdout.contains("command sent"),
            "{args:?}"
        );
    }
}

#[test]
fn firmware_palette_color_arguments_are_rejected_in_all_cli_routes() {
    for args in [
        vec!["keyboard", "breathing", "FF0000"],
        vec!["lighting", "apply", "breathing", "--color", "FF0000"],
        vec![
            "lighting",
            "save",
            "must-not-be-created",
            "breathing",
            "FF0000",
        ],
        vec!["keyboard", "neon", "FF0000"],
        vec!["keyboard", "wave", "FF0000"],
        vec!["keyboard", "shifting", "FF0000"],
        vec!["keyboard", "zoom", "FF0000"],
        vec!["keyboard", "meteor", "FF0000"],
        vec!["keyboard", "twinkling", "FF0000"],
        vec!["lighting", "apply", "rainbow", "--colors", "FF0000"],
        vec!["keyboard", "off", "FF0000"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_aps"))
            .args(&args)
            .output()
            .unwrap();
        assert!(!output.status.success(), "{args:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("no color argument"),
            "{args:?}"
        );
    }
}
