//! Root-only boot-setting/file helper. No image conversion or TUI runs elevated.
#[path = "../boot_esp.rs"]
mod boot_esp;
#[path = "../boot_media.rs"]
mod boot_media;
#[path = "../boot_policy.rs"]
mod boot_policy;
fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match boot_esp::run(&args) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Error: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
