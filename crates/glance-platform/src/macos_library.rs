//! The OS calls MaCrate needs on macOS (v0.6.0): plain bookmarks, the workspace (Open With, reveal, apps by
//! bundle id, running apps, volume and app notifications), the pasteboard, the Keychain, an FSEvents stream and
//! a file drag-out session. Each mirrors what Swift MaCrate 0.16.4 calls, with the same options.

use crate::{PlatformError, PlatformEvent, post};
use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSBitmapImageRep, NSDragOperation, NSDraggingContext, NSDraggingItem, NSDraggingSession, NSDraggingSource, NSEvent, NSEventModifierFlags, NSEventType,
    NSImage, NSPasteboard, NSPasteboardTypeFileURL, NSPasteboardTypeString, NSPasteboardWriting, NSRunningApplication, NSWorkspace, NSWorkspaceApplicationKey,
    NSWorkspaceDidLaunchApplicationNotification, NSWorkspaceDidMountNotification, NSWorkspaceDidTerminateApplicationNotification,
    NSWorkspaceDidUnmountNotification, NSWorkspaceOpenConfiguration, NSWorkspaceVolumeURLKey,
};
use objc2_core_foundation::{CFArray, CFIndex, CFRetained, CFRunLoop, CFString, kCFRunLoopDefaultMode};
#[allow(deprecated)]
use objc2_core_services::{
    ConstFSEventStreamRef, FSEventStreamContext, FSEventStreamCreate, FSEventStreamEventFlags, FSEventStreamEventId, FSEventStreamInvalidate, FSEventStreamRef,
    FSEventStreamRelease, FSEventStreamScheduleWithRunLoop, FSEventStreamStart, FSEventStreamStop, kFSEventStreamCreateFlagFileEvents,
    kFSEventStreamCreateFlagNoDefer, kFSEventStreamEventIdSinceNow,
};
use objc2_foundation::{
    NSArray, NSComparisonResult, NSData, NSNotification, NSPoint, NSRect, NSSize, NSString, NSURL, NSURLBookmarkCreationOptions, NSURLBookmarkResolutionOptions,
};
use std::cmp::Ordering;
use std::ffi::{CStr, c_char, c_void};
use std::path::{Path, PathBuf};
use std::ptr::NonNull;
use std::sync::mpsc;

fn file_url(path: &Path) -> Result<Retained<NSURL>, PlatformError> {
    NSURL::from_file_path(path).ok_or_else(|| PlatformError::Os(format!("not a file path: {}", path.display())))
}

// ---------------------------------------------------------------------------------------------
// Names

/// `NSString localizedCaseInsensitiveCompare:` (Swift MaCrate's folder and root name order).
pub fn localized_case_insensitive_compare(a: &str, b: &str) -> Ordering {
    match NSString::from_str(a).localizedCaseInsensitiveCompare(&NSString::from_str(b)) {
        NSComparisonResult::Ascending => Ordering::Less,
        NSComparisonResult::Descending => Ordering::Greater,
        _ => Ordering::Equal,
    }
}

/// Unicode canonical composition (`String.precomposedStringWithCanonicalMapping`, NFC).
pub fn precomposed(s: &str) -> String {
    NSString::from_str(s).precomposedStringWithCanonicalMapping().to_string()
}

// ---------------------------------------------------------------------------------------------
// Bookmarks

/// A resolved bookmark: the folder's path now, and whether macOS says the blob should be recreated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedBookmark {
    pub path: PathBuf,
    pub stale: bool,
}

/// A plain bookmark of `path` (`url.bookmarkData(options: [], includingResourceValuesForKeys: nil, relativeTo:
/// nil)`, no security scope), the blob Swift MaCrate keeps in `library-roots.json`.
pub fn bookmark_create(path: &Path) -> Result<Vec<u8>, PlatformError> {
    let url = file_url(path)?;
    let data = url
        .bookmarkDataWithOptions_includingResourceValuesForKeys_relativeToURL_error(NSURLBookmarkCreationOptions::empty(), None, None)
        .map_err(|e| PlatformError::Os(e.localizedDescription().to_string()))?;
    Ok(data.to_vec())
}

/// Resolve a bookmark blob (`URL(resolvingBookmarkData:options: [], relativeTo: nil, bookmarkDataIsStale:)`).
/// A folder on an unmounted volume fails with the OS's words (Swift shows that root as unavailable).
pub fn bookmark_resolve(blob: &[u8]) -> Result<ResolvedBookmark, PlatformError> {
    let data = NSData::with_bytes(blob);
    let mut stale = objc2::runtime::Bool::NO;
    // SAFETY: `data` is a valid NSData, `stale` is a valid out-pointer for the call's duration, the URL class
    // method is called with nil relative URL as documented.
    let url = unsafe {
        NSURL::URLByResolvingBookmarkData_options_relativeToURL_bookmarkDataIsStale_error(&data, NSURLBookmarkResolutionOptions::empty(), None, &mut stale)
    }
    .map_err(|e| PlatformError::Os(e.localizedDescription().to_string()))?;
    let path = url.to_file_path().ok_or_else(|| PlatformError::Os("the bookmark is not a file URL".into()))?;
    Ok(ResolvedBookmark { path, stale: stale.as_bool() })
}

// ---------------------------------------------------------------------------------------------
// Workspace

/// The applications that can open `path` (`NSWorkspace.urlsForApplications(toOpen:)`), in the OS's order. A
/// LaunchServices lookup that can block on a slow share: call off the UI thread.
pub fn apps_to_open(path: &Path) -> Result<Vec<PathBuf>, PlatformError> {
    let url = file_url(path)?;
    Ok(NSWorkspace::sharedWorkspace().URLsForApplicationsToOpenURL(&url).iter().filter_map(|u| u.to_file_path()).collect())
}

/// The bundle identifier of the app at `app` (`Bundle(url:)?.bundleIdentifier`).
pub fn bundle_id_of(app: &Path) -> Option<String> {
    let url = NSURL::from_directory_path(app)?;
    objc2_foundation::NSBundle::bundleWithURL(&url).and_then(|b| b.bundleIdentifier()).map(|s| s.to_string())
}

/// Where the app with `bundle_id` is (`NSWorkspace.urlForApplication(withBundleIdentifier:)`), if installed.
pub fn app_for_bundle_id(bundle_id: &str) -> Option<PathBuf> {
    NSWorkspace::sharedWorkspace().URLForApplicationWithBundleIdentifier(&NSString::from_str(bundle_id))?.to_file_path()
}

/// Open `paths` with the app at `app` (`NSWorkspace.open(_:withApplicationAt:configuration:)`, `activates =
/// true`). The OS answers later; a failure is not reported (Swift ignores it too).
pub fn open_with(paths: &[PathBuf], app: &Path) -> Result<(), PlatformError> {
    let urls: Vec<Retained<NSURL>> = paths.iter().map(|p| file_url(p)).collect::<Result<_, _>>()?;
    let app_url = NSURL::from_directory_path(app).ok_or_else(|| PlatformError::Os(format!("not an app path: {}", app.display())))?;
    let config = NSWorkspaceOpenConfiguration::configuration();
    config.setActivates(true);
    NSWorkspace::sharedWorkspace().openURLs_withApplicationAtURL_configuration_completionHandler(&NSArray::from_retained_slice(&urls), &app_url, &config, None);
    Ok(())
}

/// Select `paths` in a Finder window (`NSWorkspace.activateFileViewerSelecting(urls)`).
pub fn reveal_all_in_finder(paths: &[PathBuf]) -> Result<(), PlatformError> {
    let urls: Vec<Retained<NSURL>> = paths.iter().map(|p| file_url(p)).collect::<Result<_, _>>()?;
    NSWorkspace::sharedWorkspace().activateFileViewerSelectingURLs(&NSArray::from_retained_slice(&urls));
    Ok(())
}

/// Whether an app with `bundle_id` is running (`NSRunningApplication.runningApplications(withBundleIdentifier:)`).
pub fn is_app_running(bundle_id: &str) -> bool {
    !NSRunningApplication::runningApplicationsWithBundleIdentifier(&NSString::from_str(bundle_id)).is_empty()
}

/// Post [`PlatformEvent::VolumeMounted`] / [`PlatformEvent::VolumeUnmounted`] (the volume's path) and
/// [`PlatformEvent::AppLaunched`] / [`PlatformEvent::AppTerminated`] (the bundle id) from NSWorkspace's
/// notification centre, as Swift MaCrate and SendEhrKit observe them. Call once, on the main thread.
pub fn observe_workspace() -> Result<(), PlatformError> {
    MainThreadMarker::new().ok_or(PlatformError::NotMainThread)?;
    let centre = NSWorkspace::sharedWorkspace().notificationCenter();
    let volume = |n: &NSNotification| -> Option<PathBuf> {
        let info = n.userInfo()?;
        // SAFETY: NSWorkspaceVolumeURLKey is a static NSString provided by AppKit.
        let key = unsafe { NSWorkspaceVolumeURLKey };
        let value = info.objectForKey(key)?;
        value.downcast::<NSURL>().ok()?.to_file_path()
    };
    let app = |n: &NSNotification| -> Option<String> {
        let info = n.userInfo()?;
        // SAFETY: NSWorkspaceApplicationKey is a static NSString provided by AppKit.
        let key = unsafe { NSWorkspaceApplicationKey };
        let value = info.objectForKey(key)?;
        value.downcast::<NSRunningApplication>().ok()?.bundleIdentifier().map(|s| s.to_string())
    };
    let mounted = RcBlock::new(move |n: NonNull<NSNotification>| {
        // SAFETY: AppKit passes a valid notification for the block's duration.
        if let Some(p) = volume(unsafe { n.as_ref() }) {
            post(PlatformEvent::VolumeMounted(p));
        }
    });
    let unmounted = RcBlock::new(move |n: NonNull<NSNotification>| {
        // SAFETY: as above.
        if let Some(p) = volume(unsafe { n.as_ref() }) {
            post(PlatformEvent::VolumeUnmounted(p));
        }
    });
    let launched = RcBlock::new(move |n: NonNull<NSNotification>| {
        // SAFETY: as above.
        if let Some(id) = app(unsafe { n.as_ref() }) {
            post(PlatformEvent::AppLaunched(id));
        }
    });
    let terminated = RcBlock::new(move |n: NonNull<NSNotification>| {
        // SAFETY: as above.
        if let Some(id) = app(unsafe { n.as_ref() }) {
            post(PlatformEvent::AppTerminated(id));
        }
    });
    // SAFETY: the notification names are AppKit's statics; the blocks live as long as the observers, which the
    // centre retains for the life of the app (never removed).
    unsafe {
        for (name, block) in [
            (NSWorkspaceDidMountNotification, &mounted),
            (NSWorkspaceDidUnmountNotification, &unmounted),
            (NSWorkspaceDidLaunchApplicationNotification, &launched),
            (NSWorkspaceDidTerminateApplicationNotification, &terminated),
        ] {
            let observer = centre.addObserverForName_object_queue_usingBlock(Some(name), None, None, block);
            std::mem::forget(observer);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Pasteboard

/// Swift MaCrate's Copy Path: one path → `public.file-url` (the URL's absolute string) and `public.utf8-plain-text`
/// (the path); several → the paths joined by newlines as text only.
pub fn copy_paths(paths: &[PathBuf]) -> Result<(), PlatformError> {
    MainThreadMarker::new().ok_or(PlatformError::NotMainThread)?;
    let Some(first) = paths.first() else { return Ok(()) };
    let pb = NSPasteboard::generalPasteboard();
    pb.clearContents();
    // SAFETY: the pasteboard type names are AppKit's statics.
    let (string_type, url_type) = unsafe { (NSPasteboardTypeString, NSPasteboardTypeFileURL) };
    if paths.len() == 1 {
        let types = NSArray::from_slice(&[url_type, string_type]);
        // SAFETY: a nil owner is allowed.
        unsafe { pb.declareTypes_owner(&types, None) };
        let url = file_url(first)?;
        let absolute = url.absoluteString().map(|s| s.to_string()).unwrap_or_default();
        pb.setString_forType(&NSString::from_str(&first.to_string_lossy()), string_type);
        pb.setString_forType(&NSString::from_str(&absolute), url_type);
    } else {
        let types = NSArray::from_slice(&[string_type]);
        // SAFETY: a nil owner is allowed.
        unsafe { pb.declareTypes_owner(&types, None) };
        let joined: Vec<String> = paths.iter().map(|p| p.to_string_lossy().into_owned()).collect();
        pb.setString_forType(&NSString::from_str(&joined.join("\n")), string_type);
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Keychain

/// A generic password (`kSecClassGenericPassword`, service + account), as Swift MaCrate keeps the Brief API key.
pub fn keychain_get(service: &str, account: &str) -> Result<Option<String>, PlatformError> {
    match security_framework::passwords::get_generic_password(service, account) {
        Ok(bytes) => String::from_utf8(bytes).map(Some).map_err(|_| PlatformError::Os("the stored key is not text".into())),
        Err(e) if e.code() == -25300 => Ok(None),
        Err(e) => Err(PlatformError::Os(e.to_string())),
    }
}

/// Store (add or replace) a generic password.
pub fn keychain_set(service: &str, account: &str, secret: &str) -> Result<(), PlatformError> {
    security_framework::passwords::set_generic_password(service, account, secret.as_bytes()).map_err(|e| PlatformError::Os(e.to_string()))
}

/// Delete a generic password; a missing item is not an error.
pub fn keychain_delete(service: &str, account: &str) -> Result<(), PlatformError> {
    match security_framework::passwords::delete_generic_password(service, account) {
        Ok(()) => Ok(()),
        Err(e) if e.code() == -25300 => Ok(()),
        Err(e) => Err(PlatformError::Os(e.to_string())),
    }
}

// ---------------------------------------------------------------------------------------------
// FSEvents

/// One file-system event: the path FSEvents reported (not symlink-resolved) and its raw flags.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FsEvent {
    pub path: PathBuf,
    pub flags: u32,
}

/// `kFSEventStreamEventFlagUnmount`, `kFSEventStreamEventFlagRootChanged` (the two Swift inspects).
pub const FS_EVENT_FLAG_ROOT_CHANGED: u32 = 0x0000_0020;
pub const FS_EVENT_FLAG_UNMOUNT: u32 = 0x0000_0080;

/// An FSEvents stream over some folders on its own thread, configured as Swift MaCrate's watcher: file events,
/// no defer, latency 0.5 s, since now. Each callback's events are sent as one batch. Dropping it stops the stream.
pub struct FolderWatch {
    run_loop: Option<CFRetained<CFRunLoop>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

struct SendRunLoop(CFRetained<CFRunLoop>);
// SAFETY: CFRunLoop objects are thread-safe to reference and CFRunLoopStop may be called from any thread
// (Apple: "CFRunLoopStop … can be called from any thread"); this wrapper only carries the handle back.
unsafe impl Send for SendRunLoop {}

impl FolderWatch {
    /// Watch `roots` (their standardized paths); `on_batch` runs on the watcher thread.
    pub fn start(roots: &[PathBuf], latency: f64, on_batch: impl Fn(Vec<FsEvent>) + Send + 'static) -> Result<FolderWatch, PlatformError> {
        if roots.is_empty() {
            return Ok(FolderWatch { run_loop: None, thread: None });
        }
        let paths: Vec<String> = roots.iter().map(|p| p.to_string_lossy().into_owned()).collect();
        let (tx, rx) = mpsc::channel::<Result<SendRunLoop, PlatformError>>();
        let thread = std::thread::Builder::new()
            .name("glance-fsevents".into())
            .spawn(move || run_stream(paths, latency, Box::new(on_batch), tx))
            .map_err(|e| PlatformError::Os(e.to_string()))?;
        match rx.recv() {
            Ok(Ok(rl)) => Ok(FolderWatch { run_loop: Some(rl.0), thread: Some(thread) }),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(PlatformError::Os("the FSEvents thread ended".into())),
        }
    }
}

impl Drop for FolderWatch {
    fn drop(&mut self) {
        if let Some(rl) = self.run_loop.take() {
            rl.stop();
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

type Batch = Box<dyn Fn(Vec<FsEvent>) + Send>;

unsafe extern "C-unwind" fn fs_callback(
    _stream: ConstFSEventStreamRef,
    info: *mut c_void,
    count: usize,
    paths: NonNull<c_void>,
    flags: NonNull<FSEventStreamEventFlags>,
    _ids: NonNull<FSEventStreamEventId>,
) {
    // SAFETY: `info` is the `Batch` box the stream was created with and outlives the stream; FSEvents passes
    // `count` C strings in `paths` and `count` flags (no kFSEventStreamCreateFlagUseCFTypes).
    unsafe {
        let on_batch = &*(info as *const Batch);
        let paths = paths.as_ptr() as *const *const c_char;
        let flags = flags.as_ptr();
        let mut out = Vec::with_capacity(count);
        for i in 0..count {
            let p = *paths.add(i);
            if p.is_null() {
                continue;
            }
            let s = CStr::from_ptr(p).to_string_lossy().into_owned();
            out.push(FsEvent { path: PathBuf::from(s), flags: *flags.add(i) });
        }
        on_batch(out);
    }
}

#[allow(deprecated)]
fn run_stream(paths: Vec<String>, latency: f64, on_batch: Batch, tx: mpsc::Sender<Result<SendRunLoop, PlatformError>>) {
    let Some(run_loop) = CFRunLoop::current() else {
        let _ = tx.send(Err(PlatformError::Os("no run loop".into())));
        return;
    };
    let cf_paths: Vec<CFRetained<CFString>> = paths.iter().map(|p| CFString::from_str(p)).collect();
    let refs: Vec<&CFString> = cf_paths.iter().map(|s| &**s).collect();
    let array = CFArray::from_objects(&refs);
    let info = Box::into_raw(Box::new(on_batch));
    let mut context = FSEventStreamContext { version: 0 as CFIndex, info: info as *mut c_void, retain: None, release: None, copyDescription: None };
    let flags = kFSEventStreamCreateFlagFileEvents | kFSEventStreamCreateFlagNoDefer;
    // SAFETY: the callback matches FSEventStreamCallback, `context` is valid for the call (FSEvents copies it),
    // `array` holds CFStrings, `info` stays alive until after FSEventStreamRelease below.
    let stream: FSEventStreamRef =
        unsafe { FSEventStreamCreate(None, Some(fs_callback), &mut context, array.as_opaque(), kFSEventStreamEventIdSinceNow, latency, flags) };
    if stream.is_null() {
        // SAFETY: `info` came from Box::into_raw above and was not handed to a live stream.
        drop(unsafe { Box::from_raw(info) });
        let _ = tx.send(Err(PlatformError::Os("FSEventStreamCreate failed".into())));
        return;
    }
    // SAFETY: `stream` is valid; kCFRunLoopDefaultMode is CoreFoundation's static.
    let started = unsafe {
        match kCFRunLoopDefaultMode {
            Some(mode) => {
                FSEventStreamScheduleWithRunLoop(stream, &run_loop, mode);
                FSEventStreamStart(stream)
            }
            None => false,
        }
    };
    if !started {
        // SAFETY: the stream is valid and not started; `info` is freed after it is released.
        unsafe {
            FSEventStreamInvalidate(stream);
            FSEventStreamRelease(stream);
            drop(Box::from_raw(info));
        }
        let _ = tx.send(Err(PlatformError::Os("FSEventStreamStart failed".into())));
        return;
    }
    if tx.send(Ok(SendRunLoop(run_loop.clone()))).is_ok() {
        CFRunLoop::run();
    }
    // SAFETY: the run loop has returned (stopped by Drop or the send failed); the stream is valid; after release
    // no callback can run, so `info` can be freed.
    unsafe {
        FSEventStreamStop(stream);
        FSEventStreamInvalidate(stream);
        FSEventStreamRelease(stream);
        drop(Box::from_raw(info));
    }
}

// ---------------------------------------------------------------------------------------------
// Drag-out

/// One item of a file drag: the file, where its picture starts in window points (top-left origin), and its
/// picture as straight RGBA (or none: an empty frame of that size).
pub struct DragItem {
    pub path: PathBuf,
    pub frame: (f64, f64, f64, f64),
    pub rgba: Option<(u32, u32, Vec<u8>)>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "GlanceFileDragSource"]
    #[ivars = bool]
    struct FileDragSource;

    unsafe impl NSObjectProtocol for FileDragSource {}

    unsafe impl NSDraggingSource for FileDragSource {
        #[unsafe(method(draggingSession:sourceOperationMaskForDraggingContext:))]
        fn source_operation_mask(&self, _session: &NSDraggingSession, context: NSDraggingContext) -> NSDragOperation {
            if context == NSDraggingContext::OutsideApplication {
                NSDragOperation::Copy | NSDragOperation::Link
            } else if *self.ivars() {
                NSDragOperation::Move
            } else {
                NSDragOperation::None
            }
        }
    }
);

impl FileDragSource {
    fn new(mtm: MainThreadMarker, reorderable: bool) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(reorderable);
        // SAFETY: NSObject's init on a freshly allocated instance.
        unsafe { msg_send![super(this), init] }
    }
}

fn image_of(item: &DragItem) -> Retained<NSImage> {
    let size = NSSize::new(item.frame.2, item.frame.3);
    let image = NSImage::initWithSize(NSImage::alloc(), size);
    let Some((w, h, rgba)) = item.rgba.as_ref() else { return image };
    if rgba.len() != (*w as usize) * (*h as usize) * 4 || *w == 0 || *h == 0 {
        return image;
    }
    // SAFETY: a nil planes pointer makes AppKit allocate the buffer; sizes describe 8-bit RGBA, non-planar.
    let rep = unsafe {
        NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
            NSBitmapImageRep::alloc(),
            std::ptr::null_mut(),
            *w as isize,
            *h as isize,
            8,
            4,
            true,
            false,
            objc2_app_kit::NSDeviceRGBColorSpace,
            (*w as isize) * 4,
            32,
        )
    };
    let Some(rep) = rep else { return image };
    let dst = rep.bitmapData();
    if !dst.is_null() {
        // SAFETY: the rep owns a buffer of bytesPerRow * height = w*4*h bytes, the length of `rgba`.
        unsafe { std::ptr::copy_nonoverlapping(rgba.as_ptr(), dst, rgba.len()) };
    }
    rep.setSize(size);
    image.addRepresentation(&rep);
    image
}

/// Start a file drag from the window's content view with one item per file (`NSDraggingItem(pasteboardWriter:
/// url)`, plain file URLs), the operations Swift MaCrate offers (outside the app copy + link; inside the app move
/// only when `reorderable`, else none). Call while the mouse button is down (in the frame that saw the drag start).
pub fn start_file_drag(handle: raw_window_handle::RawWindowHandle, items: &[DragItem], reorderable: bool) -> Result<(), PlatformError> {
    let mtm = MainThreadMarker::new().ok_or(PlatformError::NotMainThread)?;
    if items.is_empty() {
        return Ok(());
    }
    let window = crate::macos::window_of_handle(handle)?;
    let view = window.contentView().ok_or(PlatformError::Os("the window has no content view".into()))?;
    let height = view.frame().size.height;
    let mut dragging = Vec::with_capacity(items.len());
    for item in items {
        let url = file_url(&item.path)?;
        let writer: &ProtocolObject<dyn NSPasteboardWriting> = ProtocolObject::from_ref(&*url);
        let d = NSDraggingItem::initWithPasteboardWriter(NSDraggingItem::alloc(), writer);
        let (x, y, w, h) = item.frame;
        let frame = NSRect::new(NSPoint::new(x, height - y - h), NSSize::new(w, h));
        let image = image_of(item);
        // SAFETY: `image` is an NSImage, the documented content type for a dragging frame.
        unsafe { d.setDraggingFrame_contents(frame, Some(&image)) };
        dragging.push(d);
    }
    let location = window.mouseLocationOutsideOfEventStream();
    let event = NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
        NSEventType::LeftMouseDragged,
        location,
        NSEventModifierFlags::empty(),
        0.0,
        window.windowNumber(),
        None,
        0,
        1,
        1.0,
    )
    .ok_or(PlatformError::Os("could not make the drag event".into()))?;
    let source = FileDragSource::new(mtm, reorderable);
    let source_ref: &ProtocolObject<dyn NSDraggingSource> = ProtocolObject::from_ref(&*source);
    let _session = view.beginDraggingSessionWithItems_event_source(&NSArray::from_retained_slice(&dragging), &event, source_ref);
    // The session keeps its source for the drag; keep ours alive for the app's life (one small object per drag).
    std::mem::forget(source);
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// File dates

/// When the file was added to its folder (`URLResourceKey.addedToDirectoryDateKey`), seconds since 1970 — the
/// Finder's "Date Added", Send to Arena's Sort By Date Added (v0.7.0). `None` when the volume does not record it.
pub fn date_added(path: &Path) -> Option<f64> {
    let url = file_url(path).ok()?;
    // SAFETY: NSURLAddedToDirectoryDateKey is Foundation's static key.
    let key = unsafe { objc2_foundation::NSURLAddedToDirectoryDateKey };
    let values = url.resourceValuesForKeys_error(&NSArray::from_slice(&[key])).ok()?;
    let date = values.objectForKey(key)?.downcast::<objc2_foundation::NSDate>().ok()?;
    Some(date.timeIntervalSince1970())
}
