use crate::api::types::WindowInfo;
use std::mem::size_of;
use windows::{
    core::{PCWSTR, PWSTR},
    Win32::{
        Foundation::{CloseHandle, HWND},
        Graphics::Gdi::{
            CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW,
            BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, HDC, HGDIOBJ,
        },
        System::Threading::{
            OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
            PROCESS_QUERY_LIMITED_INFORMATION,
        },
        UI::{
            Shell::ExtractIconExW,
            WindowsAndMessaging::{
                DestroyIcon, GetForegroundWindow, GetIconInfo, GetWindowTextW,
                GetWindowThreadProcessId, HICON, ICONINFO,
            },
        },
    },
};

/// Returns the current foreground window handle — single Win32 call.
pub fn frontmost_hwnd() -> u64 {
    unsafe { GetForegroundWindow().0 as u64 }
}

pub async fn current_window() -> anyhow::Result<Option<WindowInfo>> {
    tokio::task::block_in_place(collect_active_window)
}

fn collect_active_window() -> anyhow::Result<Option<WindowInfo>> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0 == 0 {
            return Ok(None);
        }

        // Window title
        let mut title_buf = vec![0u16; 512];
        let len = GetWindowTextW(hwnd, PWSTR(title_buf.as_mut_ptr()), title_buf.len() as i32);
        let title = String::from_utf16_lossy(&title_buf[..len.max(0) as usize]);

        // PID
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return Ok(None);
        }

        // Process path via OpenProcess + QueryFullProcessImageNameW
        let proc_handle = match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(h) => h,
            Err(_) => return Ok(None),
        };
        let mut path_buf = vec![0u16; 1024];
        let mut path_len = path_buf.len() as u32;
        let process_path = if QueryFullProcessImageNameW(
            proc_handle,
            PROCESS_NAME_WIN32,
            PWSTR(path_buf.as_mut_ptr()),
            &mut path_len,
        )
        .is_ok()
        {
            String::from_utf16_lossy(&path_buf[..path_len as usize])
        } else {
            String::new()
        };
        let _ = CloseHandle(proc_handle);

        let process_name = process_path
            .split('\\')
            .next_back()
            .unwrap_or("")
            .to_string();

        let icon_png = if !process_path.is_empty() {
            extract_icon_png(&process_path)
        } else {
            None
        };

        Ok(Some(WindowInfo {
            title,
            process_name,
            process_path,
            icon_png,
            url: None,
        }))
    }
}

fn extract_icon_png(exe_path: &str) -> Option<Vec<u8>> {
    unsafe {
        let wide: Vec<u16> = exe_path.encode_utf16().chain(std::iter::once(0)).collect();
        let mut hicon = HICON(0);
        let extracted = ExtractIconExW(PCWSTR(wide.as_ptr()), 0, None, Some(&mut hicon), 1);
        if extracted == 0 || hicon.0 == 0 {
            return None;
        }
        let png = hicon_to_png(hicon);
        let _ = DestroyIcon(hicon);
        png
    }
}

unsafe fn hicon_to_png(hicon: HICON) -> Option<Vec<u8>> {
    let mut info = ICONINFO::default();
    GetIconInfo(hicon, &mut info).ok()?;

    // Get bitmap dimensions
    let mut bm = BITMAP::default();
    let got = GetObjectW(
        HGDIOBJ(info.hbmColor.0),
        size_of::<BITMAP>() as i32,
        Some(&mut bm as *mut BITMAP as *mut _),
    );
    if got == 0 || bm.bmWidth <= 0 || bm.bmHeight == 0 {
        let _ = DeleteObject(HGDIOBJ(info.hbmColor.0));
        if info.hbmMask.0 != 0 {
            let _ = DeleteObject(HGDIOBJ(info.hbmMask.0));
        }
        return None;
    }
    let w = bm.bmWidth;
    let h = bm.bmHeight.abs();

    // Compatible DC needed to define color format for GetDIBits
    let hdc = CreateCompatibleDC(None);
    if hdc == HDC(0) {
        let _ = DeleteObject(HGDIOBJ(info.hbmColor.0));
        if info.hbmMask.0 != 0 {
            let _ = DeleteObject(HGDIOBJ(info.hbmMask.0));
        }
        return None;
    }

    let mut bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h, // negative = top-down scan order
            biPlanes: 1,
            biBitCount: 32,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut pixels = vec![0u8; (w * h * 4) as usize];
    GetDIBits(
        hdc,
        info.hbmColor,
        0,
        h as u32,
        Some(pixels.as_mut_ptr() as *mut _),
        &mut bmi,
        DIB_RGB_COLORS,
    );

    let _ = DeleteDC(hdc);
    let _ = DeleteObject(HGDIOBJ(info.hbmColor.0));
    if info.hbmMask.0 != 0 {
        let _ = DeleteObject(HGDIOBJ(info.hbmMask.0));
    }

    // GDI gives BGRA; image crate wants RGBA
    for chunk in pixels.chunks_exact_mut(4) {
        chunk.swap(0, 2);
    }

    let base = image::RgbaImage::from_raw(w as u32, h as u32, pixels)?;
    let img = image::DynamicImage::ImageRgba8(base);
    let img = if w as u32 > 64 || h as u32 > 64 { img.thumbnail(64, 64) } else { img };
    let mut buf = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
        .ok()?;
    Some(buf)
}
