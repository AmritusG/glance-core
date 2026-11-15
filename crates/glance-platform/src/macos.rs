//! macOS: AppKit, Foundation and MediaPlayer, as Swift Glance uses them.

use crate::{MediaCommand, NowPlaying, PlatformError, PlatformEvent, Value, post};
use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Imp, Sel};
use objc2::{AnyThread, MainThreadMarker, sel};
use objc2_app_kit::{NSDocumentController, NSWorkspace};
use objc2_foundation::{NSArray, NSBundle, NSData, NSDictionary, NSError, NSNumber, NSString, NSURL, NSUserDefaults};
use objc2_media_player::{
    MPChangePlaybackPositionCommandEvent, MPMediaItemPropertyPlaybackDuration, MPMediaItemPropertyTitle, MPNowPlayingInfoCenter,
    MPNowPlayingInfoPropertyElapsedPlaybackTime, MPNowPlayingInfoPropertyPlaybackRate, MPNowPlayingPlaybackState, MPRemoteCommand, MPRemoteCommandCenter,
    MPRemoteCommandEvent, MPRemoteCommandHandlerStatus,
};
use objc2_uniform_type_identifiers::UTType;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::ptr::NonNull;

// ---------------------------------------------------------------------------------------------
// Preferences

/// A preferences domain (`NSUserDefaults`), opened by its bundle id.
///
/// Inside the app whose bundle id is the domain this is `UserDefaults.standard` (Swift Glance's
/// store: `PlayerModel`, `AlphaTypeMemory`, `DefaultHandlerControl`); from any other process it
/// is the named suite — `UserDefaults(suiteName:)` refuses the main bundle's own id, so the two
/// cases are told apart as MaCrate's `CrateDefaults` does. Either way the values live in
/// `~/Library/Preferences/<domain>.plist` through `cfprefsd`, so the Swift app and the Rust app
/// read each other's writes.
#[derive(Debug)]
pub struct Defaults {
    store: Retained<NSUserDefaults>,
    domain: String,
}

impl Defaults {
    pub fn for_domain(domain: &str) -> Result<Defaults, PlatformError> {
        let main = NSBundle::mainBundle().bundleIdentifier().map(|s| s.to_string());
        let store = if main.as_deref() == Some(domain) {
            NSUserDefaults::standardUserDefaults()
        } else {
            NSUserDefaults::initWithSuiteName(NSUserDefaults::alloc(), Some(&NSString::from_str(domain)))
                .ok_or_else(|| PlatformError::Os(format!("no preferences domain {domain}")))?
        };
        Ok(Defaults { store, domain: domain.to_string() })
    }

    pub fn domain(&self) -> &str {
        &self.domain
    }

    /// The value stored under `key`, if any (and if it is a property-list type the family uses).
    pub fn get(&self, key: &str) -> Option<Value> {
        let object = self.store.objectForKey(&NSString::from_str(key))?;
        from_object(&object)
    }

    pub fn set(&self, key: &str, value: &Value) {
        let object = to_object(value);
        // SAFETY: `object` is a property-list object (NSString, NSNumber, NSData, NSArray or
        // NSDictionary of those, built by `to_object`), the only kind `setObject:forKey:` accepts.
        unsafe { self.store.setObject_forKey(Some(&object), &NSString::from_str(key)) };
    }

    pub fn remove(&self, key: &str) {
        self.store.removeObjectForKey(&NSString::from_str(key));
    }

    /// Write pending changes out now (cfprefsd also does it by itself; tests and harnesses that
    /// hand the domain to another process call this first).
    pub fn flush(&self) -> bool {
        self.store.synchronize()
    }
}

fn from_object(object: &AnyObject) -> Option<Value> {
    if let Some(s) = object.downcast_ref::<NSString>() {
        return Some(Value::String(s.to_string()));
    }
    if let Some(n) = object.downcast_ref::<NSNumber>() {
        // A CFBoolean is an NSNumber of its own class (`__NSCFBoolean`); a plist <true/> and
        // Swift's `set(true)` both store one.
        if n.class().name().to_bytes() == b"__NSCFBoolean" {
            return Some(Value::Bool(n.as_bool()));
        }
        // SAFETY: `objCType` returns a NUL-terminated C string owned by the number for its
        // lifetime (Foundation, "NSValue objCType"); only its first byte is read, while `n` lives.
        let kind = unsafe { *n.objCType().as_ptr() } as u8;
        return Some(if matches!(kind, b'f' | b'd') { Value::Real(n.as_f64()) } else { Value::Int(n.as_i64()) });
    }
    if let Some(d) = object.downcast_ref::<NSData>() {
        return Some(Value::Data(d.to_vec()));
    }
    if let Some(a) = object.downcast_ref::<NSArray>() {
        return a.iter().map(|o| from_object(&o)).collect::<Option<Vec<_>>>().map(Value::Array);
    }
    if let Some(d) = object.downcast_ref::<NSDictionary>() {
        let (keys, values) = d.to_vecs();
        let mut map = BTreeMap::new();
        for (k, v) in keys.iter().zip(values.iter()) {
            let k = k.downcast_ref::<NSString>()?.to_string();
            map.insert(k, from_object(v)?);
        }
        return Some(Value::Dict(map));
    }
    None
}

fn to_object(value: &Value) -> Retained<AnyObject> {
    match value {
        Value::String(s) => NSString::from_str(s).into(),
        Value::Bool(b) => NSNumber::new_bool(*b).into(),
        Value::Int(i) => NSNumber::new_i64(*i).into(),
        Value::Real(r) => NSNumber::new_f64(*r).into(),
        Value::Data(d) => NSData::with_bytes(d).into(),
        Value::Array(items) => {
            let objects: Vec<Retained<AnyObject>> = items.iter().map(to_object).collect();
            NSArray::from_retained_slice(&objects).into()
        }
        Value::Dict(map) => {
            let keys: Vec<Retained<NSString>> = map.keys().map(|k| NSString::from_str(k)).collect();
            let key_refs: Vec<&NSString> = keys.iter().map(|k| &**k).collect();
            let objects: Vec<Retained<AnyObject>> = map.values().map(to_object).collect();
            NSDictionary::<NSString, AnyObject>::from_retained_objects(&key_refs, &objects).into()
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Files sent by the OS

/// `-[NSApplicationDelegate application:openURLs:]`, added to winit's delegate.
extern "C-unwind" fn application_open_urls(_this: &AnyObject, _cmd: Sel, _app: &AnyObject, urls: &NSArray<NSURL>) {
    let paths: Vec<PathBuf> = urls.iter().filter_map(|u| u.to_file_path()).collect();
    if !paths.is_empty() {
        post(PlatformEvent::Open(paths));
    }
}

/// Deliver files the OS sends (Finder double-click, Open With, a drop on the Dock icon, `open`)
/// as [`PlatformEvent::Open`], at cold launch and while running.
///
/// Call it on the main thread **after** winit's `EventLoop::new()` and **before** `run_app`:
/// winit 0.30.13 registers its own application delegate (`WinitApplicationDelegate`, set in
/// `EventLoop::new`, whatever its module docs say) and needs it for `applicationDidFinishLaunching:`,
/// so the delegate is not replaced; `application:openURLs:` is added to its class. AppKit asks the
/// delegate whether it responds when the open event arrives — at cold launch that is during
/// `finishLaunching`, inside `run_app`, before winit's `resumed`; the paths wait in the queue.
/// Without the method AppKit would answer the Finder with "cannot open files of this type".
pub fn install_open_handler() -> Result<(), PlatformError> {
    if MainThreadMarker::new().is_none() {
        return Err(PlatformError::NotMainThread);
    }
    let class = AnyClass::get(c"WinitApplicationDelegate")
        .ok_or(PlatformError::Os("winit's application delegate class is not registered yet (call after EventLoop::new)".into()))?;
    let handler: extern "C-unwind" fn(&AnyObject, Sel, &AnyObject, &NSArray<NSURL>) = application_open_urls;
    // SAFETY: `Imp` is the erased type of every Objective-C method implementation; the runtime
    // calls it with the receiver, the selector and the two object arguments the type encoding
    // `v@:@@` states (void return, id self, SEL _cmd, id application, id urls), which is exactly
    // `application_open_urls`'s signature (`&AnyObject` / `&NSArray` are non-null object pointers,
    // as AppKit always passes for this method).
    let imp: Imp = unsafe { std::mem::transmute::<extern "C-unwind" fn(&AnyObject, Sel, &AnyObject, &NSArray<NSURL>), Imp>(handler) };
    // SAFETY: `class` is a registered class (just looked up), the selector and encoding match the
    // implementation (above); `class_addMethod` only adds, it never replaces an existing method.
    let added = unsafe { objc2::ffi::class_addMethod((class as *const AnyClass).cast_mut(), sel!(application:openURLs:), imp, c"v@:@@".as_ptr()) };
    if added.as_bool() || method_is_ours(class) {
        Ok(())
    } else {
        Err(PlatformError::Os("winit's application delegate already handles application:openURLs:".into()))
    }
}

/// A second install finds our own implementation already there.
fn method_is_ours(class: &AnyClass) -> bool {
    class.instance_method(sel!(application:openURLs:)).is_some_and(|m| m.implementation() as *const () == application_open_urls as *const ())
}

// ---------------------------------------------------------------------------------------------
// Default handler

fn content_type(uti: &str) -> Result<Retained<UTType>, PlatformError> {
    UTType::typeWithIdentifier(&NSString::from_str(uti)).ok_or_else(|| PlatformError::Os(format!("unknown content type {uti}")))
}

fn standardized(url: &NSURL) -> Option<PathBuf> {
    url.URLByStandardizingPath().and_then(|u| u.to_file_path())
}

/// Whether this app (the running bundle) is the default for content type `uti` (Swift's
/// `DefaultHandlerControl.isCurrentDefault`: `urlForApplication(toOpen:)` equals the bundle URL).
pub fn is_default_for(uti: &str) -> Result<bool, PlatformError> {
    let ty = content_type(uti)?;
    let ours = standardized(&NSBundle::mainBundle().bundleURL());
    let current = NSWorkspace::sharedWorkspace().URLForApplicationToOpenContentType(&ty);
    Ok(current.and_then(|u| standardized(&u)).is_some_and(|c| Some(c) == ours))
}

/// Ask the OS to make this app the default for content type `uti` (Swift's `setAsDefault`:
/// `NSWorkspace.setDefaultApplication(at:toOpen:)`; macOS shows its own confirmation the first
/// time). The answer arrives as [`PlatformEvent::DefaultHandlerSet`].
pub fn set_default_for(uti: &str) -> Result<(), PlatformError> {
    let ty = content_type(uti)?;
    let app = NSBundle::mainBundle().bundleURL();
    let done = RcBlock::new(|error: *mut NSError| {
        // SAFETY: the completion handler's argument is nil or a valid NSError for the duration of
        // the call (NSWorkspace documentation); it is only read here.
        let result = match unsafe { error.as_ref() } {
            None => Ok(()),
            Some(e) => Err(e.localizedDescription().to_string()),
        };
        post(PlatformEvent::DefaultHandlerSet(result));
    });
    NSWorkspace::sharedWorkspace().setDefaultApplicationAtURL_toOpenContentType_completionHandler(&app, &ty, Some(&done));
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Window subtitle

/// Set the window's subtitle (`NSWindow.subtitle`, the grey line under the title: Swift Glance's
/// `W×H · fps · duration`). `handle` is the window's raw handle (winit's `window_handle()`). Main
/// thread only.
pub fn set_subtitle(handle: raw_window_handle::RawWindowHandle, text: &str) -> Result<(), PlatformError> {
    MainThreadMarker::new().ok_or(PlatformError::NotMainThread)?;
    let raw_window_handle::RawWindowHandle::AppKit(h) = handle else {
        return Err(PlatformError::Unsupported("a non-AppKit window"));
    };
    // SAFETY: an AppKit window handle's `ns_view` is the window's live content NSView (winit's
    // contract for `window_handle()`), valid while the window exists; we are on the main thread.
    let view: &objc2_app_kit::NSView = unsafe { h.ns_view.cast::<objc2_app_kit::NSView>().as_ref() };
    let window = view.window().ok_or(PlatformError::Os("the view has no window".into()))?;
    window.setSubtitle(&NSString::from_str(text));
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Recent documents

/// Add `path` to the OS recent-documents list (Swift: `NSDocumentController.shared
/// .noteNewRecentDocumentURL`, on every open). Main thread only.
pub fn note_recent(path: &Path) -> Result<(), PlatformError> {
    let mtm = MainThreadMarker::new().ok_or(PlatformError::NotMainThread)?;
    let url = NSURL::from_file_path(path).ok_or_else(|| PlatformError::Os(format!("not a file path: {}", path.display())))?;
    NSDocumentController::sharedDocumentController(mtm).noteNewRecentDocumentURL(&url);
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Media keys and Now Playing

/// The transport media keys (F7 / F8 / F9 and headset buttons) and the Now Playing surface, as
/// Swift Glance's `NowPlayingController`: play / pause / toggle, previous / next track (= clip),
/// scrubbing from the widget; skip and seek-interval commands left off. Commands arrive as
/// [`PlatformEvent::Media`]. Publishing Now Playing info is what makes the app the one the keys go
/// to.
pub struct MediaKeys {
    targets: Vec<(Retained<MPRemoteCommand>, Retained<AnyObject>)>,
}

impl std::fmt::Debug for MediaKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MediaKeys").field("commands", &self.targets.len()).finish()
    }
}

fn add(command: Retained<MPRemoteCommand>, on: MediaCommand) -> (Retained<MPRemoteCommand>, Retained<AnyObject>) {
    let block = RcBlock::new(move |_event: NonNull<MPRemoteCommandEvent>| {
        post(PlatformEvent::Media(on));
        MPRemoteCommandHandlerStatus::Success
    });
    // SAFETY: `command` is one of the shared command centre's commands; enabling it and adding a
    // block target is its documented use. The block is copied by MediaPlayer and lives as long as
    // the target, which `MediaKeys` keeps and removes on drop.
    let target = unsafe {
        command.setEnabled(true);
        command.addTargetWithHandler(&block)
    };
    (command, target)
}

impl MediaKeys {
    pub fn register() -> Result<MediaKeys, PlatformError> {
        // SAFETY: the shared command centre and its command getters take no arguments and return
        // retained objects (MediaPlayer); called once at start-up.
        let center = unsafe { MPRemoteCommandCenter::sharedCommandCenter() };
        // SAFETY: as above, plain getters on the shared centre.
        let (toggle, play, pause, previous, next, position, off) = unsafe {
            (
                center.togglePlayPauseCommand(),
                center.playCommand(),
                center.pauseCommand(),
                center.previousTrackCommand(),
                center.nextTrackCommand(),
                center.changePlaybackPositionCommand(),
                [
                    Retained::into_super(center.skipForwardCommand()),
                    Retained::into_super(center.skipBackwardCommand()),
                    center.seekForwardCommand(),
                    center.seekBackwardCommand(),
                ],
            )
        };
        let mut targets = vec![
            add(toggle, MediaCommand::TogglePlayPause),
            add(play, MediaCommand::Play),
            add(pause, MediaCommand::Pause),
            add(previous, MediaCommand::Previous),
            add(next, MediaCommand::Next),
        ];
        let seek = RcBlock::new(|event: NonNull<MPRemoteCommandEvent>| {
            // SAFETY: MediaPlayer passes a valid event for the duration of the call; for the
            // change-playback-position command it is an `MPChangePlaybackPositionCommandEvent`.
            let event = unsafe { event.as_ref() };
            match event.downcast_ref::<MPChangePlaybackPositionCommandEvent>() {
                Some(e) => {
                    // SAFETY: a plain getter on the event.
                    post(PlatformEvent::Media(MediaCommand::Seek(unsafe { e.positionTime() })));
                    MPRemoteCommandHandlerStatus::Success
                }
                None => MPRemoteCommandHandlerStatus::CommandFailed,
            }
        });
        let position = Retained::into_super(position);
        // SAFETY: as in `add`.
        let target = unsafe {
            position.setEnabled(true);
            position.addTargetWithHandler(&seek)
        };
        targets.push((position, target));
        for command in off {
            // SAFETY: disabling a shared command (Swift leaves these off).
            unsafe { command.setEnabled(false) };
        }
        Ok(MediaKeys { targets })
    }

    /// Publish (or with `title: None` clear) what Now Playing shows (Swift's
    /// `NowPlayingController.update`): title, duration, elapsed and rate, and the playback state
    /// (macOS needs it set on every play / pause). Call on events (open, play / pause, seek); the
    /// OS moves the elapsed time on by itself from the rate.
    pub fn now_playing(&self, info: &NowPlaying) {
        // SAFETY: plain getter of the shared centre.
        let center = unsafe { MPNowPlayingInfoCenter::defaultCenter() };
        let Some(title) = info.title.as_deref() else {
            // SAFETY: nil clears the info; the state is a valid enum value.
            unsafe {
                center.setNowPlayingInfo(None);
                center.setPlaybackState(MPNowPlayingPlaybackState::Stopped);
            }
            return;
        };
        // SAFETY: the MediaPlayer key constants are immutable NSStrings exported by the framework.
        let (k_title, k_duration, k_elapsed, k_rate) = unsafe {
            (MPMediaItemPropertyTitle, MPMediaItemPropertyPlaybackDuration, MPNowPlayingInfoPropertyElapsedPlaybackTime, MPNowPlayingInfoPropertyPlaybackRate)
        };
        let mut keys: Vec<&NSString> = vec![k_title, k_elapsed, k_rate];
        let mut values: Vec<Retained<AnyObject>> = vec![
            NSString::from_str(title).into(),
            NSNumber::new_f64(info.elapsed.max(0.0)).into(),
            NSNumber::new_f64(if info.playing { 1.0 } else { 0.0 }).into(),
        ];
        if info.duration.is_finite() && info.duration > 0.0 {
            keys.push(k_duration);
            values.push(NSNumber::new_f64(info.duration).into());
        }
        let dict = NSDictionary::<NSString, AnyObject>::from_retained_objects(&keys, &values);
        // SAFETY: the dictionary maps the documented NSString keys to NSString / NSNumber values,
        // the types MediaPlayer documents for them.
        unsafe {
            center.setNowPlayingInfo(Some(&dict));
            center.setPlaybackState(if info.playing { MPNowPlayingPlaybackState::Playing } else { MPNowPlayingPlaybackState::Paused });
        }
    }
}

impl Drop for MediaKeys {
    fn drop(&mut self) {
        for (command, target) in &self.targets {
            // SAFETY: `target` is the object `addTargetWithHandler:` returned for this command.
            unsafe { command.removeTarget(Some(target)) };
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    /// Every value type the family stores goes in and comes out unchanged, through the real
    /// `cfprefsd` (a throw-away domain, removed after).
    #[test]
    fn every_plist_type_round_trips_through_a_domain() {
        let d = Defaults::for_domain("com.amritus.glancecore.platform-test").unwrap();
        let mut dict = BTreeMap::new();
        dict.insert("a".to_string(), Value::Int(1));
        dict.insert("b".to_string(), Value::Array(vec![Value::String("x".into()), Value::Bool(false)]));
        let cases = [
            ("s", Value::String("checkerboard".into())),
            ("t", Value::Bool(true)),
            ("f", Value::Bool(false)),
            ("i", Value::Int(-42)),
            ("r", Value::Real(0.75)),
            ("d", Value::Data(vec![0, 1, 2, 255])),
            ("a", Value::Array(vec![Value::String("k=premultiplied".into()), Value::String("j=straight".into())])),
            ("nested", Value::Array(vec![Value::Array(vec![Value::String("key".into()), Value::String("straight".into())])])),
            ("dict", Value::Dict(dict)),
        ];
        for (k, v) in &cases {
            d.set(k, v);
        }
        assert!(d.flush());
        let again = Defaults::for_domain("com.amritus.glancecore.platform-test").unwrap();
        for (k, v) in &cases {
            assert_eq!(again.get(k).as_ref(), Some(v), "{k}");
            again.remove(k);
        }
        assert_eq!(again.get("s"), None);
        again.flush();
    }
}
