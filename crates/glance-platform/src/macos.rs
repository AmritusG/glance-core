//! macOS: AppKit, Foundation and MediaPlayer, as Swift Glance uses them.

use crate::{AlertStyle, MediaCommand, NowPlaying, PlatformError, PlatformEvent, Value, post};
use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Imp, Sel};
use objc2::{AnyThread, MainThreadMarker, MainThreadOnly, sel};
use objc2_app_kit::{
    NSAlert, NSAlertFirstButtonReturn, NSAlertStyle, NSApplication, NSDocumentController, NSEvent, NSEventMask, NSEventModifierFlags, NSMenuItem, NSWindow,
    NSWindowCollectionBehavior, NSWindowTabbingMode, NSWorkspace,
};
use objc2_foundation::{
    NSArray, NSBundle, NSComparisonResult, NSData, NSDictionary, NSDirectoryEnumerationOptions, NSError, NSFileManager, NSNumber, NSString, NSURL,
    NSUserDefaults,
};
use objc2_media_player::{
    MPChangePlaybackPositionCommandEvent, MPMediaItemPropertyPlaybackDuration, MPMediaItemPropertyTitle, MPNowPlayingInfoCenter,
    MPNowPlayingInfoPropertyElapsedPlaybackTime, MPNowPlayingInfoPropertyPlaybackRate, MPNowPlayingPlaybackState, MPRemoteCommand, MPRemoteCommandCenter,
    MPRemoteCommandEvent, MPRemoteCommandHandlerStatus,
};
use objc2_uniform_type_identifiers::UTType;
use std::cmp::Ordering;
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
// The quit guard

/// The app has work a quit would lose (an editor's unsaved document): [`set_unsaved_work`].
static UNSAVED_WORK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Tell the quit guard ([`install_terminate_guard`]) whether a quit now would lose work.
pub fn set_unsaved_work(unsaved: bool) {
    UNSAVED_WORK.store(unsaved, std::sync::atomic::Ordering::SeqCst);
}

/// `-[NSApplicationDelegate applicationShouldTerminate:]`, added to winit's delegate: with no unsaved work the
/// quit goes ahead (`NSTerminateNow`); with unsaved work it is cancelled (`NSTerminateCancel`) and the app gets
/// [`PlatformEvent::QuitRequested`] to ask its question and quit itself.
extern "C-unwind" fn application_should_terminate(_this: &AnyObject, _cmd: Sel, _app: &AnyObject) -> usize {
    if UNSAVED_WORK.load(std::sync::atomic::Ordering::SeqCst) {
        post(PlatformEvent::QuitRequested);
        0
    } else {
        1
    }
}

/// Guard every quit that goes through AppKit's `terminate:` — the Dock's Quit, a quit Apple Event, log out, the
/// predefined Quit item — as Swift CueGlance's `applicationShouldTerminate` backstop does
/// (`CueGlanceApp.swift:220-222`). Inert until the app calls [`set_unsaved_work`]`(true)`. Same placement rules as
/// [`install_open_handler`] (main thread, after `EventLoop::new`, before `run_app`); winit 0.30.13's delegate has
/// no `applicationShouldTerminate:` of its own (`platform_impl/macos/app_state.rs`), so the method is added.
pub fn install_terminate_guard() -> Result<(), PlatformError> {
    if MainThreadMarker::new().is_none() {
        return Err(PlatformError::NotMainThread);
    }
    let class = AnyClass::get(c"WinitApplicationDelegate")
        .ok_or(PlatformError::Os("winit's application delegate class is not registered yet (call after EventLoop::new)".into()))?;
    let handler: extern "C-unwind" fn(&AnyObject, Sel, &AnyObject) -> usize = application_should_terminate;
    // SAFETY: `Imp` is the erased type of every method implementation; the runtime calls this one with the
    // receiver, the selector and the application, and reads an `NSUInteger` back, as the encoding `Q@:@` states
    // (unsigned long long return on 64-bit, id self, SEL _cmd, id sender) — `application_should_terminate`'s
    // signature (`usize` is `NSUInteger` on every Apple target this builds for).
    let imp: Imp = unsafe { std::mem::transmute::<extern "C-unwind" fn(&AnyObject, Sel, &AnyObject) -> usize, Imp>(handler) };
    // SAFETY: a registered class, a selector and encoding that match the implementation; `class_addMethod`
    // never replaces an existing method.
    let added = unsafe { objc2::ffi::class_addMethod((class as *const AnyClass).cast_mut(), sel!(applicationShouldTerminate:), imp, c"Q@:@".as_ptr()) };
    let ours =
        class.instance_method(sel!(applicationShouldTerminate:)).is_some_and(|m| m.implementation() as *const () == application_should_terminate as *const ());
    if added.as_bool() || ours { Ok(()) } else { Err(PlatformError::Os("winit's application delegate already handles applicationShouldTerminate:".into())) }
}

// ---------------------------------------------------------------------------------------------
// Menu shortcuts without a modifier

/// Offer every key press without ⌘ / ⌃ / ⌥ / ⇧ to the main menu before the window sees it, so menu
/// items whose shortcut is a bare key (Glance: Space = Play/Pause, A = Toggle Alpha Type) fire.
///
/// Why: AppKit tries a key press as a menu key equivalent only when the window's views pass it
/// up; winit 0.30.13's view takes every `keyDown:` (`interpretKeyEvents:` and its own queue,
/// `platform_impl/macos/view.rs:441-496`) and never calls `super`, so a bare-key item never fires
/// (⌘ keys work: AppKit offers those to the menu first). An `NSEvent` local monitor sees each event
/// before `sendEvent:` dispatches it (Apple: "receives copies of events the system posts to this
/// app prior to their dispatch"; returning nil "stop[s] the dispatching of the event"). The press
/// goes to `NSApp.mainMenu performKeyEquivalent:`; when an item takes it the event is dropped, so
/// the view never sees the same press (Swift: "Space lives only in the menu shortcut, or it fires
/// twice"). Repeats are offered too: a held key repeats the item as AppKit does for Swift Glance.
///
/// Not while an app-modal window is up (an alert or an open panel keeps its own keys; a sheet would
/// not be caught — the family shows none), and only in this app (local monitors see this app's
/// events only). An app with text fields would
/// need them to take the press first; the family's windows have none.
///
/// Main thread, once the application exists (after winit's `EventLoop::new`). The monitor lives for
/// the whole process.
pub fn install_menu_key_monitor() -> Result<(), PlatformError> {
    MainThreadMarker::new().ok_or(PlatformError::NotMainThread)?;
    let block = RcBlock::new(|event: NonNull<NSEvent>| -> *mut NSEvent {
        // SAFETY: AppKit passes a valid event for the duration of the call.
        let e = unsafe { event.as_ref() };
        if offered_to_menu(e) {
            return std::ptr::null_mut();
        }
        event.as_ptr()
    });
    // SAFETY: the block has the handler's signature (an event in, the event or nil out); AppKit
    // copies it and calls it on the main thread for every key-down dispatched through `sendEvent:`.
    let monitor = unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::KeyDown, &block) };
    match monitor {
        // AppKit keeps the monitor until `removeMonitor:`; this one is never removed.
        Some(m) => {
            std::mem::forget(m);
            Ok(())
        }
        None => Err(PlatformError::Os("AppKit refused the key monitor".into())),
    }
}

/// The press went to the main menu and an item took it.
fn offered_to_menu(event: &NSEvent) -> bool {
    let Some(mtm) = MainThreadMarker::new() else { return false };
    let modifiers = NSEventModifierFlags::Command | NSEventModifierFlags::Control | NSEventModifierFlags::Option | NSEventModifierFlags::Shift;
    if event.modifierFlags().intersects(modifiers) {
        return false;
    }
    let app = NSApplication::sharedApplication(mtm);
    if app.modalWindow().is_some() {
        return false;
    }
    app.mainMenu().is_some_and(|menu| menu.performKeyEquivalent(event))
}

// ---------------------------------------------------------------------------------------------
// Alerts

/// An app-modal alert in this process (`NSAlert runModal`, Swift Glance's
/// `DefaultHandlerControl.swift:90-104, 125-147`): the app's icon, `title` in bold, `text` under it
/// (none when empty), `buttons` left to right as AppKit orders them (the first is the default,
/// Return). With two buttons Esc answers the second (Glance's "Not Now"; AppKit gives Esc only to a
/// button titled "Cancel" or "Don't Save", Apple's `NSAlert addButtonWithTitle:` docs). Answers the
/// index of the button chosen; an alert ended without one (stopped by the system) is an error,
/// never read as the first button. Blocks until then; main thread only.
pub fn alert(style: AlertStyle, title: &str, text: &str, buttons: &[&str]) -> Result<usize, PlatformError> {
    let mtm = MainThreadMarker::new().ok_or(PlatformError::NotMainThread)?;
    let a = NSAlert::new(mtm);
    a.setAlertStyle(match style {
        AlertStyle::Informational => NSAlertStyle::Informational,
        AlertStyle::Warning => NSAlertStyle::Warning,
    });
    a.setMessageText(&NSString::from_str(title));
    a.setInformativeText(&NSString::from_str(text));
    for (i, b) in buttons.iter().enumerate() {
        let button = a.addButtonWithTitle(&NSString::from_str(b));
        if i == 1 && buttons.len() == 2 {
            button.setKeyEquivalent(&NSString::from_str("\u{1b}"));
        }
    }
    // AppKit's own app-modal loop; the app stays frontmost behind it.
    let response = a.runModal();
    usize::try_from(response - NSAlertFirstButtonReturn)
        .ok()
        .filter(|i| *i < buttons.len().max(1))
        .ok_or_else(|| PlatformError::Os(format!("the alert ended without a button (response {response})")))
}

// ---------------------------------------------------------------------------------------------
// Folder listing

/// Finder's order for two names (`NSString localizedStandardCompare:`, "Compares strings as sorted
/// by the Finder"; the current locale, and Apple may change it between releases).
pub fn localized_standard_compare(a: &str, b: &str) -> Ordering {
    match NSString::from_str(a).localizedStandardCompare(&NSString::from_str(b)) {
        NSComparisonResult::Ascending => Ordering::Less,
        NSComparisonResult::Descending => Ordering::Greater,
        _ => Ordering::Equal,
    }
}

/// Whether a file with extension `ext` is a movie as Swift Glance's folder scan decides
/// (`PlayerModel.swift:236-249`): the type the OS gives the lowercased extension conforms to
/// `public.movie`, `com.apple.quicktime-movie` or `public.mpeg-4`. The set is the system's.
pub fn conforms_to_movie(ext: &str) -> bool {
    let Some(ty) = UTType::typeWithFilenameExtension(&NSString::from_str(&ext.to_lowercase())) else { return false };
    ["public.movie", "com.apple.quicktime-movie", "public.mpeg-4"]
        .iter()
        .filter_map(|id| UTType::typeWithIdentifier(&NSString::from_str(id)))
        .any(|t| ty.conformsToType(&t))
}

/// The movies in `dir`, in Finder's order, as Swift Glance lists a clip's folder
/// (`PlayerModel.swift:251-268`): `contentsOfDirectory(…, options: [.skipsHiddenFiles])` (dot
/// files and files flagged hidden left out), [`conforms_to_movie`] on each extension, sorted by
/// name with [`localized_standard_compare`].
pub fn movie_files(dir: &Path) -> Result<Vec<PathBuf>, PlatformError> {
    let url = NSURL::from_directory_path(dir).ok_or_else(|| PlatformError::Os(format!("not a folder path: {}", dir.display())))?;
    let contents = NSFileManager::defaultManager()
        .contentsOfDirectoryAtURL_includingPropertiesForKeys_options_error(&url, None, NSDirectoryEnumerationOptions::SkipsHiddenFiles)
        .map_err(|e| PlatformError::Os(e.localizedDescription().to_string()))?;
    let files: Vec<PathBuf> =
        contents.iter().filter_map(|u| u.to_file_path()).filter(|p| p.extension().and_then(|e| e.to_str()).is_some_and(conforms_to_movie)).collect();
    Ok(crate::sort_by_os(&files, &|a: &PathBuf, b: &PathBuf| localized_standard_compare(&name_of(a), &name_of(b))))
}

fn name_of(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
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
    window_of(handle)?.setSubtitle(&NSString::from_str(text));
    Ok(())
}

/// The NSWindow behind a raw window handle. Main thread only.
pub(crate) fn window_of_handle(handle: raw_window_handle::RawWindowHandle) -> Result<Retained<NSWindow>, PlatformError> {
    window_of(handle)
}

fn window_of(handle: raw_window_handle::RawWindowHandle) -> Result<Retained<NSWindow>, PlatformError> {
    MainThreadMarker::new().ok_or(PlatformError::NotMainThread)?;
    let raw_window_handle::RawWindowHandle::AppKit(h) = handle else {
        return Err(PlatformError::Unsupported("a non-AppKit window"));
    };
    // SAFETY: an AppKit window handle's `ns_view` is the window's live content NSView (winit's
    // contract for `window_handle()`), valid while the window exists; we are on the main thread.
    let view: &objc2_app_kit::NSView = unsafe { h.ns_view.cast::<objc2_app_kit::NSView>().as_ref() };
    view.window().ok_or(PlatformError::Os("the view has no window".into()))
}

// ---------------------------------------------------------------------------------------------
// Window and menu rules of Swift Glance's single window

/// The app's one window as Swift Glance's SwiftUI `Window` scene makes it (read from the running
/// Swift app, a9dc601): never tabbed (`tabbingMode = .disallowed`, so AppKit adds no View ▸ Show
/// Tab Bar / Show All Tabs), not listed in the Window menu (`isExcludedFromWindowsMenu`), and
/// `collectionBehavior = [.fullScreenPrimary, .primary]` (0x10080). Main thread only.
pub fn set_single_window(handle: raw_window_handle::RawWindowHandle) -> Result<(), PlatformError> {
    let window = window_of(handle)?;
    window.setTabbingMode(NSWindowTabbingMode::Disallowed);
    window.setExcludedFromWindowsMenu(true);
    window.setCollectionBehavior(NSWindowCollectionBehavior::FullScreenPrimary | NSWindowCollectionBehavior::Primary);
    Ok(())
}

/// The show output's window (v0.6.0): above the menu bar, the Dock and notifications (`NSScreenSaverWindowLevel`), on
/// every Space, never hidden when the app is not active, not in the Window menu or the window cycle. Main thread only.
pub fn set_show_window(handle: raw_window_handle::RawWindowHandle) -> Result<(), PlatformError> {
    let window = window_of(handle)?;
    // NSScreenSaverWindowLevel (CGWindowLevelForKey(kCGScreenSaverWindowLevelKey) = 1000).
    window.setLevel(1000);
    window.setExcludedFromWindowsMenu(true);
    window.setHidesOnDeactivate(false);
    window.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::Stationary
            | NSWindowCollectionBehavior::IgnoresCycle
            | NSWindowCollectionBehavior::FullScreenAuxiliary,
    );
    Ok(())
}

static CURSOR_HIDDEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Hide or show the pointer app-wide (`NSCursor.hide` / `unhide`, kept balanced; v0.6.0). macOS hides it only while
/// the app is active. Main thread only.
pub fn set_cursor_hidden(hidden: bool) -> Result<(), PlatformError> {
    MainThreadMarker::new().ok_or(PlatformError::NotMainThread)?;
    use std::sync::atomic::Ordering;
    if CURSOR_HIDDEN.swap(hidden, Ordering::SeqCst) == hidden {
        return Ok(());
    }
    if hidden {
        objc2_app_kit::NSCursor::hide();
    } else {
        objc2_app_kit::NSCursor::unhide();
    }
    Ok(())
}

/// Save and restore the window's frame under `name` in the app's preferences (`NSWindow Frame <name>`), as a
/// SwiftUI `Window(id: name)` scene does (Swift CueGlance: `Window("CueGlance", id: "cue")` → `NSWindow Frame cue`):
/// the saved frame, when there is one, is applied now, and every later move or resize is saved. Main thread only.
pub fn set_frame_autosave(handle: raw_window_handle::RawWindowHandle, name: &str) -> Result<(), PlatformError> {
    let window = window_of(handle)?;
    let name = NSString::from_str(name);
    window.setFrameUsingName(&name);
    if window.setFrameAutosaveName(&name) { Ok(()) } else { Err(PlatformError::Os("another window already uses that frame name".into())) }
}

/// The pointer in the window's content, in points from its top-left corner (`mouseLocationOutsideOfEventStream`),
/// read when called — at a drop, the drop's point. Main thread only.
pub fn pointer_in_window(handle: raw_window_handle::RawWindowHandle) -> Result<(f64, f64), PlatformError> {
    let window = window_of(handle)?;
    let p = window.mouseLocationOutsideOfEventStream();
    let content = window.contentView().ok_or(PlatformError::Os("the window has no content view".into()))?;
    let height = content.frame().size.height;
    Ok((p.x, height - p.y))
}

/// Open a file or folder the way Finder would (`NSWorkspace.shared.open(url)`; a folder opens in a Finder
/// window, as Swift CueGlance's export-folder button does).
pub fn open_path(path: &Path) -> Result<(), PlatformError> {
    let url = NSURL::from_file_path(path).ok_or_else(|| PlatformError::Os(format!("not a file path: {}", path.display())))?;
    if NSWorkspace::sharedWorkspace().openURL(&url) { Ok(()) } else { Err(PlatformError::Os(format!("could not open {}", path.display()))) }
}

/// Show a file selected in a Finder window (`NSWorkspace.shared.activateFileViewerSelecting([url])`, GlEnc's
/// "Show" button, `JobCardView.swift:208`; v0.5.0).
pub fn reveal_in_finder(path: &Path) -> Result<(), PlatformError> {
    let url = NSURL::from_file_path(path).ok_or_else(|| PlatformError::Os(format!("not a file path: {}", path.display())))?;
    let urls = NSArray::from_retained_slice(&[url]);
    NSWorkspace::sharedWorkspace().activateFileViewerSelectingURLs(&urls);
    Ok(())
}

/// The system items Swift Glance's menu bar has from SwiftUI / AppKit, added to the menu bar the
/// app set (`NSApp.mainMenu`, Help menu set as `NSApp.helpMenu`):
/// - the app, Edit (found by its Undo or, for an editor whose Undo is its own item, its Copy) and Window menus'
///   items are enabled by AppKit's own validation
///   (`autoenablesItems`, which muda turns off on every submenu): an item is on only when something
///   in the responder chain takes its action, so with nothing to edit all Edit items are off, the
///   window answers for the items AppKit adds to the Window menu (Remove Window from Set is off, as
///   in Swift), and NSApp's own validation sets Hide Others / Show All (Show All off when no other
///   app is hidden, as in Swift; v0.3.3). The app's own items there (About, Set … as Default, Quit)
///   stay on: muda's items are their own target and take their action;
/// - Help ▸ "`app` Help" ⌘? sending `showHelp:` to the responder chain, as SwiftUI's default Help
///   menu does; with no help book AppKit's alert says "Help isn't available for `app`." (measured
///   on both apps, v0.3.2).
///
/// Call once, after the menu bar is attached. Main thread only.
pub fn finish_app_menus(app: &str) -> Result<(), PlatformError> {
    let mtm = MainThreadMarker::new().ok_or(PlatformError::NotMainThread)?;
    let ns_app = NSApplication::sharedApplication(mtm);
    let main = ns_app.mainMenu().ok_or(PlatformError::Os("no menu bar".into()))?;
    let app_menu = main
        .itemArray()
        .iter()
        .filter_map(|item| item.submenu())
        .find(|menu| menu.itemArray().iter().any(|i| i.action() == Some(sel!(unhideAllApplications:))))
        .ok_or(PlatformError::Os("no app menu (a Show All item) in the menu bar".into()))?;
    app_menu.setAutoenablesItems(true);
    let edit = main
        .itemArray()
        .iter()
        .filter_map(|item| item.submenu())
        .find(|menu| menu.itemArray().iter().any(|i| i.action() == Some(sel!(undo:)) || i.action() == Some(sel!(copy:))))
        .ok_or(PlatformError::Os("no Edit menu (an Undo or Copy item) in the menu bar".into()))?;
    edit.setAutoenablesItems(true);
    ns_app.windowsMenu().ok_or(PlatformError::Os("no Window menu set".into()))?.setAutoenablesItems(true);
    let help = ns_app.helpMenu().ok_or(PlatformError::Os("no Help menu set".into()))?;
    let title = NSString::from_str(&format!("{app} Help"));
    // SAFETY: `showHelp:` is a valid selector (NSApplication's `showHelp:`, one object argument),
    // sent through the responder chain (nil target) when the item is chosen.
    let item = unsafe { NSMenuItem::initWithTitle_action_keyEquivalent(NSMenuItem::alloc(mtm), &title, Some(sel!(showHelp:)), &NSString::from_str("?")) };
    help.addItem(&item);
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

/// The names macOS gives the attached screens ("DELL U2720Q", "Built-in Retina Display"), each with its display id
/// (`CGDirectDisplayID`, what winit's monitor handle carries on macOS). Main thread only (v0.6.0).
pub fn screen_names() -> Result<Vec<(u32, String)>, PlatformError> {
    let mtm = MainThreadMarker::new().ok_or(PlatformError::NotMainThread)?;
    let key = NSString::from_str("NSScreenNumber");
    let mut out = Vec::new();
    for screen in objc2_app_kit::NSScreen::screens(mtm).iter() {
        let description = screen.deviceDescription();
        let Some(number) = description.objectForKey(&key) else { continue };
        let Ok(number) = number.downcast::<NSNumber>() else { continue };
        out.push((number.unsignedIntValue(), screen.localizedName().to_string()));
    }
    Ok(out)
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
