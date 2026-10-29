//! Screen capture for the Windows host.
//!
//! This MVP uses the classic GDI `BitBlt` path (GetDC + CreateCompatibleDC
//! + BitBlt + GetDIBits), which is simple, has existed since Windows 2000,
//! and is the most predictable thing to get right without a compiler
//! available in this environment to check it against.
//!
//! Known limitation, flagged honestly: GDI BitBlt capture is slower than
//! the DXGI Desktop Duplication API and will not capture some
//! hardware-overlay content (certain video players, some games). Swapping
//! this module for a Desktop Duplication (IDXGIOutputDuplication) based
//! capturer is the natural next step for latency/perf, and nothing else
//! in the codebase needs to change to do that — `capture_frame` is the
//! only function the rest of the host depends on.

use anyhow::{anyhow, Result};
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
    GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
    SRCCOPY,
};
use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};

pub struct RawFrame {
    pub width: u32,
    pub height: u32,
    /// BGRA8, top-down, tightly packed rows.
    pub pixels: Vec<u8>,
}

/// Capture the full virtual primary display as a single BGRA frame.
/// Multi-monitor selection is left for a follow-up (SM_XVIRTUALSCREEN /
/// SM_CXVIRTUALSCREEN cover it when needed).
pub fn capture_frame() -> Result<RawFrame> {
    unsafe {
        let width = GetSystemMetrics(SM_CXSCREEN);
        let height = GetSystemMetrics(SM_CYSCREEN);
        if width <= 0 || height <= 0 {
            return Err(anyhow!("invalid screen metrics"));
        }

        let screen_dc = GetDC(HWND(0));
        if screen_dc.0 == 0 {
            return Err(anyhow!("GetDC failed"));
        }
        let mem_dc = CreateCompatibleDC(screen_dc);
        let bitmap = CreateCompatibleBitmap(screen_dc, width, height);
        let old_obj = SelectObject(mem_dc, bitmap);

        let blt_ok = BitBlt(mem_dc, 0, 0, width, height, screen_dc, 0, 0, SRCCOPY);

        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                // Negative height requests a top-down DIB, so rows come
                // back in normal (not upside-down) order.
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0 as u32,
                ..Default::default()
            },
            ..Default::default()
        };

        let row_bytes = (width as usize) * 4;
        let mut pixels = vec![0u8; row_bytes * height as usize];

        let got_bits = GetDIBits(
            mem_dc,
            bitmap,
            0,
            height as u32,
            Some(pixels.as_mut_ptr() as *mut _),
            &mut bmi,
            DIB_RGB_COLORS,
        );

        SelectObject(mem_dc, old_obj);
        let _ = DeleteObject(bitmap);
        let _ = DeleteDC(mem_dc);
        ReleaseDC(HWND(0), screen_dc);

        if blt_ok.as_bool() == false {
            return Err(anyhow!("BitBlt failed"));
        }
        if got_bits == 0 {
            return Err(anyhow!("GetDIBits failed"));
        }

        Ok(RawFrame {
            width: width as u32,
            height: height as u32,
            pixels,
        })
    }
}

/// Encode a raw BGRA frame as JPEG for transport. JPEG is used as a
/// pragmatic placeholder codec: it is simple, has no external codec
/// dependency, and is good enough to validate the whole pipeline
/// end-to-end. For production-grade motion video, replace this with a
/// real video codec (H.264 via Media Foundation hardware encode is the
/// natural choice on Windows) — `capture.rs` and `protocol::VideoChunk`
/// do not need to change to support that, only this function does.
pub fn encode_jpeg(frame: &RawFrame, quality: u8) -> Result<Vec<u8>> {
    use image::{ImageBuffer, Rgba};

    // BGRA -> RGBA
    let mut rgba = vec![0u8; frame.pixels.len()];
    for px in frame.pixels.chunks_exact(4).zip(rgba.chunks_exact_mut(4)) {
        let (src, dst) = px;
        dst[0] = src[2];
        dst[1] = src[1];
        dst[2] = src[0];
        dst[3] = 255;
    }

    let img: ImageBuffer<Rgba<u8>, _> =
        ImageBuffer::from_raw(frame.width, frame.height, rgba)
            .ok_or_else(|| anyhow!("frame buffer size mismatch"))?;

    let mut out = Vec::new();
    let mut encoder =
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality);
    encoder.encode_image(&img)?;
    Ok(out)
}
