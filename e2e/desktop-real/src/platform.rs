//! Platform-specific resolution of the release binary, the native WebDriver,
//! and the display server. No hardcoded user-home paths.

use std::path::{Path, PathBuf};
use std::process::{Child, Command};

/// How the native driver binary is located.
pub enum NativeDriver {
    /// Explicit path resolved from env var or a well-known machine location.
    Explicit(PathBuf),
    /// Let tauri-driver auto-detect the driver on PATH.
    Auto,
}

/// Resolve the native browser driver for the current platform.
///
/// Windows (product E2E): Edge WebDriver for WebView2.
/// - `LINGBI_EDGE_DRIVER` env var wins, then the GitHub Actions
///   well-known install location, then tauri-driver auto-detection.
///
/// Linux (compat only): WebKitWebDriver.
/// - `LINGBI_WEBKIT_DRIVER` env var wins, then tauri-driver auto-detection.
pub fn native_driver() -> NativeDriver {
    #[cfg(target_os = "windows")]
    {
        if let Ok(path) = std::env::var("LINGBI_EDGE_DRIVER")
            && !path.is_empty()
        {
            return NativeDriver::Explicit(PathBuf::from(path));
        }
        let well_known =
            PathBuf::from(r"C:\Program Files (x86)\Microsoft\Edge\Application\msedgedriver.exe");
        if well_known.exists() {
            return NativeDriver::Explicit(well_known);
        }
        NativeDriver::Auto
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(path) = std::env::var("LINGBI_WEBKIT_DRIVER")
            && !path.is_empty()
        {
            return NativeDriver::Explicit(PathBuf::from(path));
        }
        NativeDriver::Auto
    }
}

/// Whether an executable is reachable on PATH.
fn on_path(name: &str) -> bool {
    let Some(path_var) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path_var).any(|dir| dir.join(name).is_file())
}

/// Pre-flight infra check. Returns `Err(reason)` when an E2E cannot even
/// start on this machine, so callers can decide between a hard failure
/// (Windows product gate) and an explicit compat-skip (Linux compat).
pub fn check_infra() -> Result<(), String> {
    if !on_path("tauri-driver") {
        return Err("tauri-driver not found on PATH (cargo install tauri-driver)".to_owned());
    }
    match native_driver() {
        NativeDriver::Explicit(path) => {
            if !path.exists() {
                return Err(format!(
                    "configured native driver missing at {}",
                    path.display()
                ));
            }
        }
        NativeDriver::Auto => {
            #[cfg(target_os = "windows")]
            if !on_path("msedgedriver.exe") {
                return Err(
                    "msedgedriver.exe not found on PATH (set LINGBI_EDGE_DRIVER)".to_owned(),
                );
            }
            #[cfg(target_os = "linux")]
            if !on_path("WebKitWebDriver") {
                return Err(
                    "WebKitWebDriver not found on PATH (set LINGBI_WEBKIT_DRIVER)".to_owned(),
                );
            }
        }
    }
    #[cfg(target_os = "linux")]
    if !on_path("Xvfb") {
        return Err("Xvfb not found on PATH".to_owned());
    }
    Ok(())
}

/// Absolute path of the LingBi desktop release binary for this platform.
pub fn release_binary(repo: &Path) -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        repo.join("apps/desktop/src-tauri/target/release/lingbi-desktop.exe")
    }
    #[cfg(not(target_os = "windows"))]
    {
        repo.join("apps/desktop/src-tauri/target/release/lingbi-desktop")
    }
}

/// Process guard that kills the child on drop.
pub struct ChildGuard(Child);

impl ChildGuard {
    pub fn new(child: Child) -> Self {
        Self(child)
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Display server guard. On Windows the desktop session provides the
/// display and this is a no-op; on Linux an Xvfb is spawned (compat only).
pub struct DisplayGuard {
    #[cfg(target_os = "linux")]
    _xvfb: Option<ChildGuard>,
    #[cfg(target_os = "linux")]
    display: Option<String>,
}

impl DisplayGuard {
    /// Environment value that child processes need to reach the display.
    #[cfg(target_os = "linux")]
    pub fn display_env(&self) -> Option<&str> {
        self.display.as_deref()
    }

    #[cfg(not(target_os = "linux"))]
    pub fn display_env(&self) -> Option<&str> {
        None
    }
}

/// Start the display server if the platform needs one.
///
/// Linux: spawns Xvfb on a TCP-only display so CI does not depend on a
/// writable /tmp/.X11-unix socket. Returns `Err` with the reason when
/// Xvfb is unavailable so the caller can decide (compat skip vs failure).
pub fn start_display() -> Result<DisplayGuard, String> {
    #[cfg(target_os = "linux")]
    {
        let server_display = ":97";
        let client_display = "127.0.0.1:97";
        let xvfb = Command::new("Xvfb")
            .args([
                server_display,
                "-screen",
                "0",
                "1280x800x24",
                "-nolisten",
                "unix",
                "-nolisten",
                "local",
                "-listen",
                "tcp",
            ])
            .spawn()
            .map_err(|error| format!("Xvfb unavailable: {error}"))?;
        Ok(DisplayGuard {
            _xvfb: Some(ChildGuard::new(xvfb)),
            display: Some(client_display.to_owned()),
        })
    }
    #[cfg(not(target_os = "linux"))]
    {
        Ok(DisplayGuard {})
    }
}

/// Pick a free TCP port on 127.0.0.1.
pub fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("free port");
    let port = listener.local_addr().expect("local addr").port();
    drop(listener);
    port
}

/// Build the tauri-driver command for this platform on the given ports.
pub fn tauri_driver_command(driver: &NativeDriver, port: u16, native_port: u16) -> Command {
    let mut command = Command::new("tauri-driver");
    command.args([
        "--port",
        &port.to_string(),
        "--native-port",
        &native_port.to_string(),
    ]);
    if let NativeDriver::Explicit(path) = driver {
        command.args(["--native-driver", path.to_string_lossy().as_ref()]);
    }
    #[cfg(target_os = "linux")]
    {
        command.env("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
    }
    command
}
