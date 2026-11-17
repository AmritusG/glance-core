//! Unattended running (v0.6.0, for GlancEhr): keep the screen awake while a show plays, and start the app at login.
//!
//! - [`StayAwake`]: macOS `IOPMAssertionCreateWithName(PreventUserIdleDisplaySleep)` (no display sleep, no idle
//!   screensaver; `pmset -g assertions` lists it); Windows `PowerCreateRequest` + `PowerSetRequest` for the display and
//!   the system (Microsoft: `SetThreadExecutionState` alone does not stop the screensaver; a borderless full-screen
//!   window does). Released when dropped.
//! - [`login_item`] / [`set_login_item`]: macOS `SMAppService.mainApp` (macOS 13+; a bundled app; the user may have to
//!   approve it in System Settings ▸ Login Items); Windows the per-user `Run` key, with the state Windows' own Startup
//!   switch (`StartupApproved\Run`) gives it read, never changed.

use crate::PlatformError;

/// Whether the app starts at login.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoginItem {
    Off,
    On,
    /// Registered, but the user has to allow it (macOS System Settings ▸ Login Items; Windows Settings ▸ Startup).
    NeedsApproval,
}

#[cfg(target_os = "macos")]
mod imp {
    use super::LoginItem;
    use crate::PlatformError;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, AnyObject};
    use objc2::{msg_send, sel};
    use objc2_core_foundation::CFString;
    use objc2_foundation::NSError;

    #[link(name = "IOKit", kind = "framework")]
    unsafe extern "C" {
        fn IOPMAssertionCreateWithName(kind: *const CFString, level: u32, name: *const CFString, id: *mut u32) -> i32;
        fn IOPMAssertionRelease(id: u32) -> i32;
    }

    // SMAppService lives in ServiceManagement; linking the framework registers the class.
    #[link(name = "ServiceManagement", kind = "framework")]
    unsafe extern "C" {}

    /// kIOPMAssertionLevelOn.
    const LEVEL_ON: u32 = 255;

    pub struct StayAwake(u32);

    impl StayAwake {
        pub fn new(reason: &str) -> Result<StayAwake, PlatformError> {
            let kind = CFString::from_str("PreventUserIdleDisplaySleep");
            let name = CFString::from_str(reason);
            let mut id = 0u32;
            // SAFETY: both strings are live CFStrings for the call (`CFRetained` keeps them), `id` is a valid out
            // pointer; the function copies what it needs and returns an IOReturn.
            let r = unsafe { IOPMAssertionCreateWithName(&*kind, LEVEL_ON, &*name, &mut id) };
            if r == 0 { Ok(StayAwake(id)) } else { Err(PlatformError::Os(format!("IOPMAssertionCreateWithName returned {r:#x}"))) }
        }
    }

    impl Drop for StayAwake {
        fn drop(&mut self) {
            // SAFETY: the id came from a successful IOPMAssertionCreateWithName and is released once.
            let _ = unsafe { IOPMAssertionRelease(self.0) };
        }
    }

    fn main_app() -> Result<Retained<AnyObject>, PlatformError> {
        let class = AnyClass::get(c"SMAppService").ok_or(PlatformError::Unsupported("SMAppService (macOS 13 or later)"))?;
        if class.class_method(sel!(mainAppService)).is_none() {
            return Err(PlatformError::Unsupported("SMAppService.mainAppService"));
        }
        // SAFETY: `+[SMAppService mainAppService]` takes no arguments and returns the (autoreleased) service object.
        let service: Option<Retained<AnyObject>> = unsafe { msg_send![class, mainAppService] };
        service.ok_or(PlatformError::Os("no main-app service (is the app bundled?)".into()))
    }

    pub fn login_item() -> Result<LoginItem, PlatformError> {
        let s = main_app()?;
        // SAFETY: `-[SMAppService status]` takes no arguments and returns an NSInteger (SMAppServiceStatus).
        let status: isize = unsafe { msg_send![&*s, status] };
        Ok(match status {
            1 => LoginItem::On,
            2 => LoginItem::NeedsApproval,
            _ => LoginItem::Off,
        })
    }

    pub fn set_login_item(on: bool) -> Result<LoginItem, PlatformError> {
        let s = main_app()?;
        let r: Result<(), Retained<NSError>> = if on {
            // SAFETY: `registerAndReturnError:` takes an NSError** and returns BOOL; objc2's `_` form passes the out
            // pointer and turns NO into the error.
            unsafe { msg_send![&*s, registerAndReturnError: _] }
        } else {
            // SAFETY: as above, `unregisterAndReturnError:`.
            unsafe { msg_send![&*s, unregisterAndReturnError: _] }
        };
        match r {
            Ok(()) => login_item(),
            Err(e) => {
                let state = login_item();
                if on && state == Ok(LoginItem::NeedsApproval) { state } else { Err(PlatformError::Os(e.localizedDescription().to_string())) }
            }
        }
    }
}

#[cfg(windows)]
mod imp {
    use super::LoginItem;
    use crate::PlatformError;
    use windows::Win32::Foundation::{CloseHandle, ERROR_SUCCESS, HANDLE};
    use windows::Win32::System::Power::{PowerClearRequest, PowerCreateRequest, PowerRequestDisplayRequired, PowerRequestSystemRequired, PowerSetRequest};
    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_BINARY, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW};
    use windows::Win32::System::Threading::{POWER_REQUEST_CONTEXT_SIMPLE_STRING, REASON_CONTEXT, REASON_CONTEXT_0};
    use windows::core::{HSTRING, PWSTR};

    const RUN: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
    const APPROVED: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved\\Run";

    pub struct StayAwake(HANDLE);

    impl StayAwake {
        pub fn new(reason: &str) -> Result<StayAwake, PlatformError> {
            let mut text: Vec<u16> = reason.encode_utf16().chain(std::iter::once(0)).collect();
            let context = REASON_CONTEXT {
                Version: 0, // POWER_REQUEST_CONTEXT_VERSION
                Flags: POWER_REQUEST_CONTEXT_SIMPLE_STRING,
                Reason: REASON_CONTEXT_0 { SimpleReasonString: PWSTR(text.as_mut_ptr()) },
            };
            // SAFETY: `context` and the string it points to live for the call; Windows copies the reason.
            let handle = unsafe { PowerCreateRequest(&context) }.map_err(|e| PlatformError::Os(e.to_string()))?;
            // SAFETY: a request handle just created; the requests are cleared and the handle closed on drop.
            unsafe {
                PowerSetRequest(handle, PowerRequestDisplayRequired).map_err(|e| PlatformError::Os(e.to_string()))?;
                PowerSetRequest(handle, PowerRequestSystemRequired).map_err(|e| PlatformError::Os(e.to_string()))?;
            }
            Ok(StayAwake(handle))
        }
    }

    impl Drop for StayAwake {
        fn drop(&mut self) {
            // SAFETY: the handle is the one PowerCreateRequest returned; cleared and closed once.
            unsafe {
                let _ = PowerClearRequest(self.0, PowerRequestDisplayRequired);
                let _ = PowerClearRequest(self.0, PowerRequestSystemRequired);
                let _ = CloseHandle(self.0);
            }
        }
    }

    fn value_name() -> HSTRING {
        let name = std::env::current_exe().ok().and_then(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned())).unwrap_or_else(|| "GlancEhr".into());
        HSTRING::from(name)
    }

    pub fn login_item() -> Result<LoginItem, PlatformError> {
        let name = value_name();
        let mut size = 0u32;
        // SAFETY: a size query (no buffer) of a value under the current user's Run key.
        let present = unsafe { RegGetValueW(HKEY_CURRENT_USER, &HSTRING::from(RUN), &name, RRF_RT_REG_SZ, None, None, Some(&mut size)) } == ERROR_SUCCESS;
        if !present {
            return Ok(LoginItem::Off);
        }
        let mut data = [0u8; 12];
        let mut len = data.len() as u32;
        // SAFETY: reads at most `len` bytes into `data` (Windows' 12-byte StartupApproved record).
        let read = unsafe {
            RegGetValueW(HKEY_CURRENT_USER, &HSTRING::from(APPROVED), &name, RRF_RT_REG_BINARY, None, Some(data.as_mut_ptr().cast()), Some(&mut len))
        };
        // First byte 03: switched off in Windows' Startup settings (a user's choice we never override).
        Ok(if read == ERROR_SUCCESS && data[0] == 3 { LoginItem::NeedsApproval } else { LoginItem::On })
    }

    pub fn set_login_item(on: bool) -> Result<LoginItem, PlatformError> {
        let name = value_name();
        let r = if on {
            let exe = std::env::current_exe().map_err(|e| PlatformError::Os(e.to_string()))?;
            let command: Vec<u16> = format!("\"{}\"", exe.display()).encode_utf16().chain(std::iter::once(0)).collect();
            // SAFETY: writes a REG_SZ of `command` (NUL-terminated UTF-16, its byte length given) under HKCU's Run key.
            unsafe { RegSetKeyValueW(HKEY_CURRENT_USER, &HSTRING::from(RUN), &name, REG_SZ.0, Some(command.as_ptr().cast()), (command.len() * 2) as u32) }
        } else {
            // SAFETY: removes the app's own value from HKCU's Run key.
            unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, &HSTRING::from(RUN), &name) }
        };
        if r != ERROR_SUCCESS && on {
            return Err(PlatformError::Os(format!("the Run key could not be written ({})", r.0)));
        }
        login_item()
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
mod imp {
    use super::LoginItem;
    use crate::PlatformError;

    pub struct StayAwake;

    impl StayAwake {
        pub fn new(_reason: &str) -> Result<StayAwake, PlatformError> {
            Err(PlatformError::Unsupported("keeping the screen awake (iOS: the app's idle timer)"))
        }
    }

    pub fn login_item() -> Result<LoginItem, PlatformError> {
        Err(PlatformError::Unsupported("login items"))
    }

    pub fn set_login_item(_on: bool) -> Result<LoginItem, PlatformError> {
        Err(PlatformError::Unsupported("login items"))
    }
}

/// While this lives, the display does not sleep and the screensaver does not start (dropping it gives that back).
pub struct StayAwake(#[allow(dead_code)] imp::StayAwake);

impl StayAwake {
    pub fn new(reason: &str) -> Result<StayAwake, PlatformError> {
        imp::StayAwake::new(reason).map(StayAwake)
    }
}

/// Whether the app starts at login now.
pub fn login_item() -> Result<LoginItem, PlatformError> {
    imp::login_item()
}

/// Start the app at login, or stop; returns the state after the change.
pub fn set_login_item(on: bool) -> Result<LoginItem, PlatformError> {
    imp::set_login_item(on)
}
