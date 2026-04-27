use crate::api::types::WindowInfo;
use x11rb::{
    connection::Connection,
    protocol::xproto::{AtomEnum, ConnectionExt as _},
    rust_connection::RustConnection,
};

pub async fn current_window() -> anyhow::Result<Option<WindowInfo>> {
    tokio::task::block_in_place(collect_active_window)
}

fn collect_active_window() -> anyhow::Result<Option<WindowInfo>> {
    let (conn, screen_num) = match RustConnection::connect(None) {
        Ok(c) => c,
        Err(_) => return Ok(None),
    };
    let root = conn.setup().roots[screen_num].root;

    let net_active_window = conn.intern_atom(false, b"_NET_ACTIVE_WINDOW")?.reply()?.atom;
    let net_wm_name = conn.intern_atom(false, b"_NET_WM_NAME")?.reply()?.atom;
    let wm_name_atom = conn.intern_atom(false, b"WM_NAME")?.reply()?.atom;
    let net_wm_pid = conn.intern_atom(false, b"_NET_WM_PID")?.reply()?.atom;
    let net_wm_icon = conn.intern_atom(false, b"_NET_WM_ICON")?.reply()?.atom;
    let utf8_string = conn.intern_atom(false, b"UTF8_STRING")?.reply()?.atom;

    // Active window ID
    let aw = conn
        .get_property(false, root, net_active_window, AtomEnum::WINDOW, 0, 1)?
        .reply()?;
    if aw.value.len() < 4 {
        return Ok(None);
    }
    let win: u32 = u32::from_ne_bytes(aw.value[..4].try_into().unwrap());
    if win == 0 || win == root {
        return Ok(None);
    }

    // Title: _NET_WM_NAME (UTF-8) first, WM_NAME fallback
    let title = get_text_prop(&conn, win, net_wm_name, utf8_string)
        .or_else(|_| get_text_prop(&conn, win, wm_name_atom, AtomEnum::STRING.into()))
        .unwrap_or_default();

    // PID via _NET_WM_PID
    let pid_r = conn
        .get_property(false, win, net_wm_pid, AtomEnum::CARDINAL, 0, 1)?
        .reply()?;
    let pid: u32 = if pid_r.value.len() >= 4 {
        u32::from_ne_bytes(pid_r.value[..4].try_into().unwrap())
    } else {
        0
    };

    let (process_name, process_path) = if pid > 0 {
        let path = std::fs::read_link(format!("/proc/{pid}/exe"))
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        let name = std::fs::read_to_string(format!("/proc/{pid}/comm"))
            .map(|s| s.trim().to_owned())
            .unwrap_or_default();
        (name, path)
    } else {
        (String::new(), String::new())
    };

    let icon_png = wm_icon_to_png(&conn, win, net_wm_icon);

    Ok(Some(WindowInfo {
        title,
        process_name,
        process_path,
        icon_png,
        url: None,
    }))
}

fn get_text_prop(conn: &RustConnection, win: u32, prop: u32, type_: u32) -> anyhow::Result<String> {
    let r = conn
        .get_property(false, win, prop, type_, 0, 2048)?
        .reply()?;
    anyhow::ensure!(!r.value.is_empty(), "empty property");
    Ok(String::from_utf8_lossy(&r.value).into_owned())
}

// _NET_WM_ICON contains one or more icons: [w, h, w*h ARGB u32s, ...].
// Pick the largest and encode as PNG.
fn wm_icon_to_png(conn: &RustConnection, win: u32, net_wm_icon: u32) -> Option<Vec<u8>> {
    let r = conn
        .get_property(false, win, net_wm_icon, AtomEnum::CARDINAL, 0, 1 << 20)
        .ok()?
        .reply()
        .ok()?;

    let words: Vec<u32> = r
        .value
        .chunks_exact(4)
        .map(|b| u32::from_ne_bytes(b.try_into().unwrap()))
        .collect();

    // Walk icon list, remember offset of the largest one
    let mut best: Option<(usize, usize, usize)> = None; // (w, h, data_offset)
    let mut i = 0;
    while i + 2 <= words.len() {
        let w = words[i] as usize;
        let h = words[i + 1] as usize;
        if w == 0 || h == 0 {
            break;
        }
        let end = i + 2 + w * h;
        if end > words.len() {
            break;
        }
        if best.map_or(true, |(bw, bh, _)| w * h > bw * bh) {
            best = Some((w, h, i + 2));
        }
        i = end;
    }
    let (w, h, off) = best?;

    // ARGB → RGBA
    let rgba: Vec<u8> = words[off..off + w * h]
        .iter()
        .flat_map(|&px| {
            let a = (px >> 24) as u8;
            let r = (px >> 16) as u8;
            let g = (px >> 8) as u8;
            let b = px as u8;
            [r, g, b, a]
        })
        .collect();

    let base = image::RgbaImage::from_raw(w as u32, h as u32, rgba)?;
    let img = image::DynamicImage::ImageRgba8(base);
    let img = if w > 64 || h > 64 { img.thumbnail(64, 64) } else { img };
    let mut buf = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
        .ok()?;
    Some(buf)
}
