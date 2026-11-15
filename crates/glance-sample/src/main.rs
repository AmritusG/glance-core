//! glance-sample: the core's shell in one bundled app (macOS; compiles for Windows). See [`app`].

#[cfg(any(target_os = "macos", windows))]
mod app;

#[cfg(any(target_os = "macos", windows))]
fn main() -> std::process::ExitCode {
    app::main()
}

#[cfg(not(any(target_os = "macos", windows)))]
fn main() {
    eprintln!("glance-sample runs on macOS and Windows");
}
