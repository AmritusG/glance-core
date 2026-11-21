//! A Syphon server (macOS): the app's pictures offered to other software on this Mac (Resolume, MadMapper, OBS's
//! Syphon client, …). Syphon.framework (BSD-3-style licence, Tom Butterworth & Anton Marini; its Metal server by Maxime
//! Touroute & Philippe Chaurand) is not linked: it is loaded at run time from the app bundle's `Contents/Frameworks`
//! (or `GLANCE_SYPHON_FRAMEWORK`), so an app without it builds and runs, and says Syphon is missing. The picture is
//! handed over on wgpu's own Metal command queue, no copy: the texture's Metal object and a command buffer of that
//! queue go to `-[SyphonMetalServer publishFrameTexture:onCommandBuffer:imageRegion:flipped:]`, which copies it into
//! the shared IOSurface on the GPU.

use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyClass, AnyObject, Bool, ProtocolObject};
use objc2::{Message, msg_send};
use objc2_foundation::{NSBundle, NSPoint, NSRect, NSSize, NSString};
use objc2_metal::{MTLCommandBuffer, MTLCommandQueue, MTLDevice, MTLTexture};

/// Where Syphon.framework is looked for, in order.
fn candidates() -> Vec<String> {
    let mut v = Vec::new();
    if let Ok(p) = std::env::var("GLANCE_SYPHON_FRAMEWORK") {
        v.push(p);
    }
    if let Some(p) = NSBundle::mainBundle().privateFrameworksPath() {
        v.push(format!("{p}/Syphon.framework"));
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        v.push(dir.join("../Frameworks/Syphon.framework").display().to_string());
        v.push(dir.join("Syphon.framework").display().to_string());
    }
    v
}

/// Load Syphon.framework once; `Err` says where it was looked for.
pub fn syphon_available() -> Result<(), String> {
    if AnyClass::get(c"SyphonMetalServer").is_some() {
        return Ok(());
    }
    let places = candidates();
    for p in &places {
        if !std::path::Path::new(p).exists() {
            continue;
        }
        let Some(b) = NSBundle::bundleWithPath(&NSString::from_str(p)) else { continue };
        // SAFETY: loading a framework bundle runs its initialisers; Syphon.framework is the BSD framework the app
        // ships in its own bundle (or the path a measured run names), loaded once.
        if unsafe { b.load() } && AnyClass::get(c"SyphonMetalServer").is_some() {
            return Ok(());
        }
    }
    Err(format!("Syphon.framework was not found (looked in {})", places.join(", ")))
}

pub struct SyphonServer {
    server: Retained<AnyObject>,
    queue: Retained<ProtocolObject<dyn MTLCommandQueue>>,
}

impl SyphonServer {
    /// A server called `name` on the wgpu device's Metal device, publishing on its queue.
    pub fn new(name: &str, device: &wgpu::Device, queue: &wgpu::Queue) -> Result<SyphonServer, String> {
        syphon_available()?;
        let cls = AnyClass::get(c"SyphonMetalServer").ok_or("Syphon has no SyphonMetalServer")?;
        // SAFETY: the hal device is only read (its MTLDevice retained) while `device` is borrowed.
        let mtl: Retained<ProtocolObject<dyn MTLDevice>> =
            unsafe { device.as_hal::<wgpu::hal::api::Metal>() }.map(|d| d.raw_device().clone()).ok_or("the GPU is not a Metal device")?;
        // SAFETY: the same for the queue: its MTLCommandQueue is retained and used only to make command buffers.
        let q: Retained<ProtocolObject<dyn MTLCommandQueue>> =
            unsafe { queue.as_hal::<wgpu::hal::api::Metal>() }.map(|q| q.as_raw().retain()).ok_or("the GPU queue is not a Metal queue")?;
        let nsname = NSString::from_str(name);
        // SAFETY: +alloc on the class Syphon defines, then its documented initialiser
        // `initWithName:(NSString*)device:(id<MTLDevice>)options:(NSDictionary*)`, which returns nil on failure.
        let server: Option<Retained<AnyObject>> = unsafe {
            let a: Allocated<AnyObject> = msg_send![cls, alloc];
            msg_send![a, initWithName: &*nsname, device: &*mtl, options: Option::<&AnyObject>::None]
        };
        let server = server.ok_or("Syphon could not start a server")?;
        Ok(SyphonServer { server, queue: q })
    }

    /// Whether any client is watching (publishing can be skipped otherwise).
    pub fn has_clients(&self) -> bool {
        // SAFETY: `hasClients` is a BOOL property of SyphonServerBase.
        let b: Bool = unsafe { msg_send![&*self.server, hasClients] };
        b.as_bool()
    }

    /// Publish `texture` (its whole area). Call after the frame that drew it was submitted on the same queue.
    pub fn publish(&self, texture: &wgpu::Texture) -> Result<(), String> {
        // SAFETY: the hal texture is read only while `texture` is borrowed; its MTLTexture goes to Syphon, which
        // copies it on the command buffer below before this returns its encoding.
        let hal = unsafe { texture.as_hal::<wgpu::hal::api::Metal>() }.ok_or("the picture is not a Metal texture")?;
        let raw: &ProtocolObject<dyn MTLTexture> = hal.raw_handle();
        let cb = self.queue.commandBuffer().ok_or("no Metal command buffer")?;
        let region = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(f64::from(texture.width()), f64::from(texture.height())));
        // SAFETY: Syphon's documented method; the texture and the command buffer live past the call; we commit it.
        unsafe {
            let _: () = msg_send![&*self.server, publishFrameTexture: raw, onCommandBuffer: &*cb, imageRegion: region, flipped: Bool::NO];
        }
        cb.commit();
        Ok(())
    }
}

impl Drop for SyphonServer {
    fn drop(&mut self) {
        // SAFETY: `stop` is SyphonMetalServer's documented method; the object stays retained until after it.
        unsafe {
            let _: () = msg_send![&*self.server, stop];
        }
    }
}
