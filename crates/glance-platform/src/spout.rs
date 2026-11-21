//! A Spout sender (Windows): the app's pictures offered to other software on this PC (Resolume, TouchDesigner, OBS's
//! Spout plugin, …), speaking Spout 2's sender protocol as Spout2's own SpoutSenderNames / SpoutSharedMemory /
//! SpoutDirectX write it (github.com/leadedge/Spout2, BSD-2-Clause, Lynn Jarvis; read, not copied):
//!
//! - the picture is a shareable D3D11 texture (`D3D11_RESOURCE_MISC_SHARED`, `DXGI_FORMAT_R8G8B8A8_UNORM` = 28) whose
//!   legacy share handle receivers open;
//! - the sender's name is in the shared memory "SpoutSenderNames" (MaxSenders × 256-byte NUL-terminated names, sorted,
//!   an empty name ending the list; MaxSenders from `HKCU\Software\Leading Edge\Spout` or 64), guarded by the mutex
//!   "SpoutSenderNames_mutex", and made the "ActiveSenderName" (256 bytes);
//! - the shared memory named after the sender holds its `SharedTextureInfo` (280 bytes: share handle, width, height,
//!   format, usage, 256-byte description, partner id), guarded by "<name>_mutex";
//! - writes to the texture happen under "<name>_SpoutAccessMutex" (receivers wait on it while they copy).
//!
//! The picture arrives as RGBA8 bytes (the app reads its frame back); copying it into the texture is one
//! `UpdateSubresource` per frame (1080p fits a frame; a zero-copy D3D12 → D3D11 path can follow).

use windows::Win32::Foundation::{CloseHandle, HANDLE, HMODULE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0};
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_11_0};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_RESOURCE_MISC_SHARED, D3D11_SDK_VERSION,
    D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT, D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::IDXGIResource;
use windows::Win32::System::Memory::{CreateFileMappingA, FILE_MAP_ALL_ACCESS, MEMORY_MAPPED_VIEW_ADDRESS, MapViewOfFile, PAGE_READWRITE, UnmapViewOfFile};
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueA};
use windows::Win32::System::Threading::{CreateMutexA, ReleaseMutex, WaitForSingleObject};
use windows::core::{Interface, PCSTR};

const NAME_LEN: usize = 256;
const INFO_LEN: usize = 280;

fn cstr(s: &str) -> Vec<u8> {
    let mut v: Vec<u8> = s.bytes().filter(|b| *b != 0).collect();
    v.push(0);
    v
}

/// A named shared memory block and its mutex (SpoutSharedMemory's pair).
struct Shm {
    map: HANDLE,
    view: MEMORY_MAPPED_VIEW_ADDRESS,
    mutex: HANDLE,
    size: usize,
}

impl Shm {
    fn open(name: &str, size: usize) -> Result<Shm, String> {
        let n = cstr(name);
        // SAFETY: a pagefile-backed mapping with a NUL-terminated name that lives for the call.
        let map =
            unsafe { CreateFileMappingA(INVALID_HANDLE_VALUE, None, PAGE_READWRITE, 0, size as u32, PCSTR(n.as_ptr())) }.map_err(|e| format!("{name}: {e}"))?;
        // SAFETY: `map` is a valid mapping handle; the whole section is mapped.
        let view = unsafe { MapViewOfFile(map, FILE_MAP_ALL_ACCESS, 0, 0, size) };
        if view.Value.is_null() {
            // SAFETY: the handle came from CreateFileMappingA and is closed once.
            let _ = unsafe { CloseHandle(map) };
            return Err(format!("{name}: the shared memory does not map"));
        }
        let m = cstr(&format!("{name}_mutex"));
        // SAFETY: a named mutex, not owned at creation; the name lives for the call.
        let mutex = match unsafe { CreateMutexA(None, false, PCSTR(m.as_ptr())) } {
            Ok(h) => h,
            Err(e) => {
                // SAFETY: the view and handle above are released once.
                unsafe {
                    let _ = UnmapViewOfFile(view);
                    let _ = CloseHandle(map);
                }
                return Err(format!("{name}: {e}"));
            }
        };
        Ok(Shm { map, view, mutex, size })
    }

    /// Run `f` on the block while holding its mutex (Spout waits 67 ms).
    fn with<R>(&self, f: impl FnOnce(&mut [u8]) -> R) -> Option<R> {
        // SAFETY: `mutex` is a live mutex handle.
        if unsafe { WaitForSingleObject(self.mutex, 67) } != WAIT_OBJECT_0 {
            return None;
        }
        // SAFETY: the view maps `size` bytes of the section and stays mapped while `self` lives; the mutex keeps other
        // Spout processes out while the slice exists.
        let buf = unsafe { std::slice::from_raw_parts_mut(self.view.Value.cast::<u8>(), self.size) };
        let r = f(buf);
        // SAFETY: this thread owns the mutex (the wait above succeeded).
        let _ = unsafe { ReleaseMutex(self.mutex) };
        Some(r)
    }
}

impl Drop for Shm {
    fn drop(&mut self) {
        // SAFETY: each handle and the view are released once, here.
        unsafe {
            let _ = UnmapViewOfFile(self.view);
            let _ = CloseHandle(self.map);
            let _ = CloseHandle(self.mutex);
        }
    }
}

/// The sender names list in a "SpoutSenderNames" block: 256-byte NUL-terminated names until an empty one.
pub fn read_names(buf: &[u8]) -> Vec<String> {
    buf.chunks(NAME_LEN)
        .map_while(|c| {
            let end = c.iter().position(|b| *b == 0).unwrap_or(c.len());
            (end > 0).then(|| String::from_utf8_lossy(&c[..end]).into_owned())
        })
        .collect()
}

/// Write `names` (sorted, as Spout's std::set keeps them) into the block, ending the list with an empty name.
pub fn write_names(buf: &mut [u8], names: &[String]) {
    let mut sorted: Vec<&String> = names.iter().collect();
    sorted.sort();
    sorted.dedup();
    let slots = buf.len() / NAME_LEN;
    for (i, chunk) in buf.chunks_mut(NAME_LEN).enumerate() {
        chunk.fill(0);
        if let Some(n) = sorted.get(i).filter(|_| i < slots) {
            let b = n.as_bytes();
            let k = b.len().min(NAME_LEN - 1);
            chunk[..k].copy_from_slice(&b[..k]);
        }
    }
}

/// A sender's `SharedTextureInfo` (280 bytes, little-endian u32 fields).
pub fn texture_info(handle: u32, width: u32, height: u32, format: u32, description: &str) -> [u8; INFO_LEN] {
    let mut v = [0u8; INFO_LEN];
    for (i, x) in [handle, width, height, format, 0].iter().enumerate() {
        v[i * 4..i * 4 + 4].copy_from_slice(&x.to_le_bytes());
    }
    let d = description.as_bytes();
    let k = d.len().min(255);
    v[20..20 + k].copy_from_slice(&d[..k]);
    v
}

fn max_senders() -> usize {
    let mut v: u32 = 0;
    let mut len = 4u32;
    let key = cstr("Software\\Leading Edge\\Spout");
    let val = cstr("MaxSenders");
    // SAFETY: reads one DWORD into `v` (4 bytes, `len` says so); the strings live for the call.
    let r =
        unsafe { RegGetValueA(HKEY_CURRENT_USER, PCSTR(key.as_ptr()), PCSTR(val.as_ptr()), RRF_RT_REG_DWORD, None, Some((&raw mut v).cast()), Some(&mut len)) };
    if r.is_ok() && (1..=1000).contains(&v) { v as usize } else { 64 }
}

pub struct SpoutSender {
    pub name: String,
    device: ID3D11Device,
    ctx: ID3D11DeviceContext,
    texture: Option<(ID3D11Texture2D, (u32, u32))>,
    names: Shm,
    info: Option<Shm>,
    access: HANDLE,
}

impl SpoutSender {
    /// A sender called `name` (a taken name gets `_1`, `_2`… as Spout does).
    pub fn new(name: &str) -> Result<SpoutSender, String> {
        let (mut device, mut ctx) = (None, None);
        // SAFETY: a hardware D3D11 device on the default adapter; the out pointers are locals.
        unsafe {
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                Some(&[D3D_FEATURE_LEVEL_11_0]),
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut ctx),
            )
        }
        .map_err(|e| format!("Direct3D 11: {e}"))?;
        let (device, ctx) = (device.ok_or("no Direct3D 11 device")?, ctx.ok_or("no Direct3D 11 context")?);
        let names = Shm::open("SpoutSenderNames", max_senders() * NAME_LEN)?;
        let mut chosen = name.to_string();
        names
            .with(|buf| {
                let mut list = read_names(buf);
                let mut i = 1;
                while list.contains(&chosen) {
                    chosen = format!("{name}_{i}");
                    i += 1;
                }
                list.push(chosen.clone());
                write_names(buf, &list);
            })
            .ok_or("the Spout sender list is busy")?;
        if let Ok(active) = Shm::open("ActiveSenderName", NAME_LEN) {
            active.with(|b| {
                b.fill(0);
                let k = chosen.len().min(NAME_LEN - 1);
                b[..k].copy_from_slice(&chosen.as_bytes()[..k]);
            });
        }
        let am = cstr(&format!("{chosen}_SpoutAccessMutex"));
        // SAFETY: a named mutex, not owned at creation.
        let access = unsafe { CreateMutexA(None, false, PCSTR(am.as_ptr())) }.map_err(|e| e.to_string())?;
        Ok(SpoutSender { name: chosen, device, ctx, texture: None, names, info: None, access })
    }

    /// Send one picture (`rgba`: `width` × `height` RGBA8 rows, tightly packed).
    pub fn send(&mut self, rgba: &[u8], width: u32, height: u32) -> Result<(), String> {
        if rgba.len() < (width as usize) * (height as usize) * 4 || width == 0 || height == 0 {
            return Err("the picture is smaller than its size".into());
        }
        if self.texture.as_ref().is_none_or(|(_, s)| *s != (width, height)) {
            let desc = D3D11_TEXTURE2D_DESC {
                Width: width,
                Height: height,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32,
                CPUAccessFlags: 0,
                MiscFlags: D3D11_RESOURCE_MISC_SHARED.0 as u32,
            };
            let mut tex = None;
            // SAFETY: `desc` describes a valid 2D texture; the out pointer is a local.
            unsafe { self.device.CreateTexture2D(&desc, None, Some(&mut tex)) }.map_err(|e| format!("the Spout texture: {e}"))?;
            let tex = tex.ok_or("no Spout texture")?;
            let res: IDXGIResource = tex.cast().map_err(|e| e.to_string())?;
            // SAFETY: a texture made with D3D11_RESOURCE_MISC_SHARED has a legacy share handle.
            let handle = unsafe { res.GetSharedHandle() }.map_err(|e| e.to_string())?;
            let info = Shm::open(&self.name, INFO_LEN)?;
            let rec = texture_info(handle.0 as usize as u32, width, height, DXGI_FORMAT_R8G8B8A8_UNORM.0 as u32, "GlancEhr");
            info.with(|b| b[..INFO_LEN].copy_from_slice(&rec)).ok_or("the Spout sender record is busy")?;
            self.info = Some(info);
            self.texture = Some((tex, (width, height)));
        }
        let Some((tex, _)) = &self.texture else { return Err("no Spout texture".into()) };
        // SAFETY: `access` is a live mutex; receivers hold it while they copy the texture.
        if unsafe { WaitForSingleObject(self.access, 67) } != WAIT_OBJECT_0 {
            return Ok(());
        }
        // SAFETY: `rgba` holds `height` rows of `width * 4` bytes (checked above); the texture is that size.
        unsafe {
            self.ctx.UpdateSubresource(tex, 0, None, rgba.as_ptr().cast(), width * 4, 0);
            self.ctx.Flush();
            let _ = ReleaseMutex(self.access);
        }
        Ok(())
    }
}

impl Drop for SpoutSender {
    fn drop(&mut self) {
        let me = self.name.clone();
        self.names.with(|buf| {
            let list: Vec<String> = read_names(buf).into_iter().filter(|n| *n != me).collect();
            write_names(buf, &list);
        });
        // SAFETY: the access mutex is closed once.
        let _ = unsafe { CloseHandle(self.access) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_names_list_and_the_record_match_spouts_layout() {
        let mut buf = vec![0u8; 4 * NAME_LEN];
        write_names(&mut buf, &["Resolume".into(), "GlancEhr".into()]);
        assert_eq!(&buf[..8], b"GlancEhr", "sorted");
        assert_eq!(read_names(&buf), vec!["GlancEhr".to_string(), "Resolume".to_string()]);
        assert_eq!(buf[2 * NAME_LEN], 0, "an empty name ends the list");
        let r = texture_info(0x1234, 1920, 1080, 28, "GlancEhr");
        assert_eq!(r.len(), 280);
        assert_eq!(&r[0..4], &0x1234u32.to_le_bytes());
        assert_eq!(&r[4..8], &1920u32.to_le_bytes());
        assert_eq!(&r[12..16], &28u32.to_le_bytes());
        assert_eq!(&r[20..28], b"GlancEhr");
    }
}
