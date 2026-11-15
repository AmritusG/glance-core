//! Windows: one instance per app, a second launch's files handed to the first, and the System
//! Media Transport Controls. Written and compile-checked on the Mac; first run at the Windows
//! milestone (C4, docs/00-ROADMAP.md §7).

use crate::{MediaCommand, NowPlaying, PlatformError, PlatformEvent, post};
use std::io::Write;
use std::path::PathBuf;
use windows::Foundation::{TimeSpan, TypedEventHandler};
use windows::Media::{
    MediaPlaybackStatus, MediaPlaybackType, PlaybackPositionChangeRequestedEventArgs, SystemMediaTransportControls, SystemMediaTransportControlsButton,
    SystemMediaTransportControlsButtonPressedEventArgs, SystemMediaTransportControlsTimelineProperties,
};
use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, HWND, INVALID_HANDLE_VALUE};
use windows::Win32::Storage::FileSystem::{PIPE_ACCESS_INBOUND, ReadFile};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::System::WinRT::ISystemMediaTransportControlsInterop;
use windows::core::{HSTRING, PCWSTR};

fn os(e: windows::core::Error) -> PlatformError {
    PlatformError::Os(e.message())
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// This app's one instance (Windows starts a new process per double-click; macOS sends the files
/// to the running app by itself).
#[derive(Debug)]
pub enum SingleInstance {
    /// The first instance: it owns the lock, and a thread queues the paths later launches send
    /// as [`PlatformEvent::Open`].
    Primary(PrimaryLock),
    /// Another instance runs and has been sent this launch's paths: exit now.
    Handed,
}

/// The lock the first instance holds for its lifetime.
#[derive(Debug)]
pub struct PrimaryLock {
    mutex: HANDLE,
}

impl Drop for PrimaryLock {
    fn drop(&mut self) {
        // SAFETY: `mutex` is the handle `CreateMutexW` returned to this instance, closed once.
        let _ = unsafe { CloseHandle(self.mutex) };
    }
}

impl SingleInstance {
    /// Take the per-user lock `Local\<app_id>`. When it is free this is the primary instance and
    /// `\\.\pipe\<app_id>` starts listening; when another instance holds it, `paths` (this launch's
    /// command-line files) are written to that pipe, one UTF-8 path per line, and the caller exits.
    pub fn acquire(app_id: &str, paths: &[PathBuf]) -> Result<SingleInstance, PlatformError> {
        let name = wide(&format!("Local\\{app_id}"));
        // SAFETY: `name` is a NUL-terminated UTF-16 string that outlives the call; no security
        // attributes (the default), not initially owned.
        let mutex = unsafe { CreateMutexW(None, false, PCWSTR(name.as_ptr())) }.map_err(os)?;
        // SAFETY: reads the calling thread's last-error value, set by `CreateMutexW` just above.
        let exists = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
        let pipe = format!(r"\\.\pipe\{app_id}");
        if exists {
            // SAFETY: our handle to the existing mutex, not needed any more.
            let _ = unsafe { CloseHandle(mutex) };
            let mut client = std::fs::OpenOptions::new().write(true).open(&pipe).map_err(|e| PlatformError::Os(format!("{pipe}: {e}")))?;
            for p in paths {
                writeln!(client, "{}", p.display()).map_err(|e| PlatformError::Os(e.to_string()))?;
            }
            return Ok(SingleInstance::Handed);
        }
        std::thread::Builder::new().name("glance-single-instance".into()).spawn(move || listen(&pipe)).map_err(|e| PlatformError::Os(e.to_string()))?;
        Ok(SingleInstance::Primary(PrimaryLock { mutex }))
    }
}

/// Serve the pipe forever: each client's lines are one [`PlatformEvent::Open`].
fn listen(pipe: &str) {
    let name = wide(pipe);
    loop {
        // SAFETY: `name` is NUL-terminated UTF-16 and outlives the call; an inbound byte pipe,
        // blocking, default security.
        let handle = unsafe {
            CreateNamedPipeW(
                PCWSTR(name.as_ptr()),
                PIPE_ACCESS_INBOUND,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
                PIPE_UNLIMITED_INSTANCES,
                0,
                64 * 1024,
                0,
                None,
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return;
        }
        // SAFETY: `handle` is the pipe just created; synchronous connect (no OVERLAPPED). A client
        // that connected between create and connect makes it fail with ERROR_PIPE_CONNECTED, which
        // is fine to read from.
        let _ = unsafe { ConnectNamedPipe(handle, None) };
        let mut bytes = Vec::new();
        let mut buf = [0u8; 4096];
        loop {
            let mut read = 0u32;
            // SAFETY: `buf` is a live buffer of its own length; `read` receives the count; the
            // handle is the connected pipe; synchronous read.
            let ok = unsafe { ReadFile(handle, Some(&mut buf), Some(&mut read), None) };
            if ok.is_err() || read == 0 {
                break;
            }
            bytes.extend_from_slice(buf.get(..read as usize).unwrap_or_default());
        }
        let paths: Vec<PathBuf> = String::from_utf8_lossy(&bytes).lines().filter(|l| !l.is_empty()).map(PathBuf::from).collect();
        if !paths.is_empty() {
            post(PlatformEvent::Open(paths));
        }
        // SAFETY: the pipe instance served above, disconnected and closed once.
        unsafe {
            let _ = DisconnectNamedPipe(handle);
            let _ = CloseHandle(handle);
        }
    }
}

/// Attach `menu` as the window's menu bar (muda's `init_for_hwnd`, which installs its accelerator
/// hook on the window). `handle` is the window's raw handle (winit's `window_handle()`).
pub fn attach_menu(menu: &muda::Menu, handle: raw_window_handle::RawWindowHandle) -> Result<(), PlatformError> {
    let raw_window_handle::RawWindowHandle::Win32(h) = handle else {
        return Err(PlatformError::Unsupported("a non-Win32 window"));
    };
    // SAFETY: `hwnd` is the app's live top-level window (winit's handle), on its own thread, which
    // is muda's contract for `init_for_hwnd`.
    unsafe { menu.init_for_hwnd(h.hwnd.get()) }.map_err(|e| PlatformError::Os(e.to_string()))
}

/// The System Media Transport Controls of the app's window: play / pause / previous / next and
/// the timeline (the media flyout, keyboard media keys), as [`PlatformEvent::Media`].
pub struct MediaKeys {
    smtc: SystemMediaTransportControls,
}

impl std::fmt::Debug for MediaKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MediaKeys")
    }
}

/// A .NET `TimeSpan` (100 ns units) of `seconds`.
fn span(seconds: f64) -> TimeSpan {
    TimeSpan { Duration: (seconds.max(0.0) * 1e7) as i64 }
}

impl MediaKeys {
    /// Register for the window `hwnd` (the raw `HWND` from winit's window handle).
    pub fn register(hwnd: isize) -> Result<MediaKeys, PlatformError> {
        let interop = windows::core::factory::<SystemMediaTransportControls, ISystemMediaTransportControlsInterop>().map_err(os)?;
        // SAFETY: `hwnd` is the app's live top-level window (the caller's contract); the interop
        // returns the controls object bound to it.
        let smtc: SystemMediaTransportControls = unsafe { interop.GetForWindow(HWND(hwnd as *mut core::ffi::c_void)) }.map_err(os)?;
        smtc.SetIsEnabled(true).map_err(os)?;
        smtc.SetIsPlayEnabled(true).map_err(os)?;
        smtc.SetIsPauseEnabled(true).map_err(os)?;
        smtc.SetIsPreviousEnabled(true).map_err(os)?;
        smtc.SetIsNextEnabled(true).map_err(os)?;
        smtc.ButtonPressed(&TypedEventHandler::<SystemMediaTransportControls, SystemMediaTransportControlsButtonPressedEventArgs>::new(|_, args| {
            if let Some(args) = args.as_ref() {
                let command = match args.Button()? {
                    SystemMediaTransportControlsButton::Play => Some(MediaCommand::Play),
                    SystemMediaTransportControlsButton::Pause => Some(MediaCommand::Pause),
                    SystemMediaTransportControlsButton::Previous => Some(MediaCommand::Previous),
                    SystemMediaTransportControlsButton::Next => Some(MediaCommand::Next),
                    _ => None,
                };
                if let Some(c) = command {
                    post(PlatformEvent::Media(c));
                }
            }
            Ok(())
        }))
        .map_err(os)?;
        smtc.PlaybackPositionChangeRequested(&TypedEventHandler::<SystemMediaTransportControls, PlaybackPositionChangeRequestedEventArgs>::new(|_, args| {
            if let Some(args) = args.as_ref() {
                let at = args.RequestedPlaybackPosition()?;
                post(PlatformEvent::Media(MediaCommand::Seek(at.Duration as f64 / 1e7)));
            }
            Ok(())
        }))
        .map_err(os)?;
        Ok(MediaKeys { smtc })
    }

    /// Publish (or with `title: None` clear) the title, timeline and playback state.
    pub fn now_playing(&self, info: &NowPlaying) {
        let _ = self.update(info);
    }

    fn update(&self, info: &NowPlaying) -> windows::core::Result<()> {
        let display = self.smtc.DisplayUpdater()?;
        let Some(title) = info.title.as_deref() else {
            display.ClearAll()?;
            display.Update()?;
            return self.smtc.SetPlaybackStatus(MediaPlaybackStatus::Stopped);
        };
        display.SetType(MediaPlaybackType::Video)?;
        display.VideoProperties()?.SetTitle(&HSTRING::from(title))?;
        display.Update()?;
        let timeline = SystemMediaTransportControlsTimelineProperties::new()?;
        timeline.SetStartTime(span(0.0))?;
        timeline.SetMinSeekTime(span(0.0))?;
        let end = if info.duration.is_finite() && info.duration > 0.0 { info.duration } else { 0.0 };
        timeline.SetEndTime(span(end))?;
        timeline.SetMaxSeekTime(span(end))?;
        timeline.SetPosition(span(info.elapsed))?;
        self.smtc.UpdateTimelineProperties(&timeline)?;
        self.smtc.SetPlaybackStatus(if info.playing { MediaPlaybackStatus::Playing } else { MediaPlaybackStatus::Paused })
    }
}
