use crate::api::types::WindowInfo;
use objc2::rc::{autoreleasepool, Retained};
use objc2::runtime::AnyObject;
use objc2_app_kit::{
    NSBitmapImageFileType, NSBitmapImageRep, NSBitmapImageRepPropertyKey, NSImage, NSWorkspace,
};
use objc2_foundation::NSDictionary;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

// ── Screen Recording permission ──────────────────────────────────────────────

static SCREEN_CAPTURE_INIT: OnceLock<()> = OnceLock::new();

fn ensure_screen_capture_permission() {
    SCREEN_CAPTURE_INIT.get_or_init(|| unsafe {
        extern "C" {
            fn CGPreflightScreenCaptureAccess() -> bool;
            fn CGRequestScreenCaptureAccess() -> bool;
        }
        if !CGPreflightScreenCaptureAccess() {
            CGRequestScreenCaptureAccess();
        }
    });
}

// ── Icon cache ───────────────────────────────────────────────────────────────
// NSImage → PNG encode is expensive (TIFF intermediate, ~1 MB alloc).
// App icons never change at runtime, so encode once per bundle_id.

fn icon_cache() -> &'static Mutex<HashMap<String, Option<Vec<u8>>>> {
    static C: OnceLock<Mutex<HashMap<String, Option<Vec<u8>>>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

fn cached_icon_png(bundle_id: &str, image: &NSImage) -> Option<Vec<u8>> {
    {
        let cache = icon_cache().lock().unwrap();
        if let Some(entry) = cache.get(bundle_id) {
            return entry.clone();
        }
    }
    let png = unsafe { ns_image_to_png(image) };
    icon_cache().lock().unwrap().insert(bundle_id.to_owned(), png.clone());
    png
}

// ── URL throttle ─────────────────────────────────────────────────────────────
// osascript spawns a 20-30 MB subprocess. At 400 ms poll intervals a slow
// AppleScript call can leave 2-3 concurrent processes alive. Cap to 1 call/s.

struct UrlState {
    bundle_id: String,
    url: Option<String>,
    fetched_at: Option<Instant>,
}

fn url_state() -> &'static Mutex<UrlState> {
    static S: OnceLock<Mutex<UrlState>> = OnceLock::new();
    S.get_or_init(|| {
        Mutex::new(UrlState {
            bundle_id: String::new(),
            url: None,
            fetched_at: None,
        })
    })
}

fn throttled_browser_url(bundle_id: &str) -> Option<String> {
    if browser_script_target(bundle_id).is_none() {
        return None;
    }
    let now = Instant::now();
    {
        let state = url_state().lock().unwrap();
        if state.bundle_id == bundle_id {
            if let Some(t) = state.fetched_at {
                if t.elapsed() < Duration::from_millis(1000) {
                    return state.url.clone();
                }
            }
        }
    }
    let url = fetch_browser_url(bundle_id);
    let mut state = url_state().lock().unwrap();
    state.bundle_id = bundle_id.to_owned();
    state.url = url.clone();
    state.fetched_at = Some(now);
    url
}

pub async fn current_window() -> anyhow::Result<Option<WindowInfo>> {
    tokio::task::block_in_place(|| autoreleasepool(|_| collect_active_window()))
}

fn collect_active_window() -> anyhow::Result<Option<WindowInfo>> {
    ensure_screen_capture_permission();
    unsafe {
        let workspace = NSWorkspace::sharedWorkspace();
        let Some(app) = workspace.frontmostApplication() else {
            return Ok(None);
        };

        let process_name = app
            .localizedName()
            .map(|s| s.to_string())
            .unwrap_or_default();

        let process_path = app
            .bundleURL()
            .and_then(|url| url.path())
            .map(|s| s.to_string())
            .unwrap_or_default();

        let bundle_id = app
            .bundleIdentifier()
            .map(|s| s.to_string())
            .unwrap_or_default();

        let pid: i32 = objc2::msg_send![&*app, processIdentifier];
        let title = window_title_for_pid(pid).unwrap_or_default();
        let icon_png = app.icon().and_then(|img| cached_icon_png(&bundle_id, &img));
        let url = throttled_browser_url(&bundle_id);

        Ok(Some(WindowInfo {
            title,
            process_name,
            process_path,
            icon_png,
            url: url, // keyingi bosqichda
        }))
    }
}

/// NSImage'ni PNG bayt'larga aylantirish (TIFF -> NSBitmapImageRep -> PNG)
unsafe fn ns_image_to_png(image: &NSImage) -> Option<Vec<u8>> {
    let tiff = image.TIFFRepresentation()?;
    let rep = NSBitmapImageRep::imageRepWithData(&tiff)?;

    let props: Retained<NSDictionary<NSBitmapImageRepPropertyKey, AnyObject>> = NSDictionary::new();

    let png = rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &props)?;

    let len = png.length();
    let bytes_ptr = png.bytes();
    let slice = std::slice::from_raw_parts(bytes_ptr.as_ptr().cast::<u8>(), len);

    // macOS icons often 512×512 or larger; scale to 64×64 max (1 MB → 16 KB texture)
    match image::load_from_memory(slice) {
        Ok(img) if img.width() > 64 || img.height() > 64 => {
            let resized = img.thumbnail(64, 64);
            let mut buf = Vec::new();
            resized
                .write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
                .ok()?;
            Some(buf)
        }
        _ => Some(slice.to_vec()),
    }
}

/// CGWindowListCopyWindowInfo orqali shu PID ga tegishli eng oldindagi
/// (eng past layer) ko'rinuvchi oynaning sarlavhasini topish
fn window_title_for_pid(target_pid: i32) -> Option<String> {
    use core_foundation::{
        array::{CFArrayGetCount, CFArrayGetValueAtIndex, CFArrayRef},
        base::{CFRelease, TCFType, ToVoid},
        dictionary::{CFDictionaryGetValue, CFDictionaryRef},
        number::{kCFNumberSInt32Type, kCFNumberSInt64Type, CFNumberGetValue, CFNumberRef},
        string::{CFString, CFStringRef},
    };
    use core_graphics::window::{
        kCGNullWindowID, kCGWindowListExcludeDesktopElements, kCGWindowListOptionOnScreenOnly,
        CGWindowListCopyWindowInfo,
    };

    unsafe {
        let array: CFArrayRef = CGWindowListCopyWindowInfo(
            kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements,
            kCGNullWindowID,
        );
        if array.is_null() {
            return None;
        }

        let pid_key = CFString::from_static_string("kCGWindowOwnerPID");
        let layer_key = CFString::from_static_string("kCGWindowLayer");
        let name_key = CFString::from_static_string("kCGWindowName");

        let count = CFArrayGetCount(array);
        let mut best: Option<(i64, String)> = None;

        for i in 0..count {
            let dict = CFArrayGetValueAtIndex(array, i) as CFDictionaryRef;
            if dict.is_null() {
                continue;
            }

            // PID — frontmost app PID bilan solishtiramiz
            let pid_val = CFDictionaryGetValue(dict, pid_key.to_void()) as CFNumberRef;
            if pid_val.is_null() {
                continue;
            }
            let mut pid: i32 = 0;
            CFNumberGetValue(pid_val, kCFNumberSInt32Type, (&mut pid as *mut i32).cast());
            if pid != target_pid {
                continue;
            }

            // Layer (kichikroq = oldindaroq)
            let layer_val = CFDictionaryGetValue(dict, layer_key.to_void()) as CFNumberRef;
            let mut layer: i64 = i64::MAX;
            if !layer_val.is_null() {
                CFNumberGetValue(
                    layer_val,
                    kCFNumberSInt64Type,
                    (&mut layer as *mut i64).cast(),
                );
            }

            // Title
            let name_val = CFDictionaryGetValue(dict, name_key.to_void()) as CFStringRef;
            if name_val.is_null() {
                continue;
            }
            let name = CFString::wrap_under_get_rule(name_val).to_string();
            if name.is_empty() {
                continue;
            }

            if best.as_ref().map_or(true, |(l, _)| layer < *l) {
                best = Some((layer, name));
            }
        }

        CFRelease(array.cast());
        best.map(|(_, n)| n)
    }
}

/// Bundle ID → (app nomi, tab terminologiyasi)
/// Safari "current tab" deydi, Chromium-asosli brauzerlar "active tab".
fn browser_script_target(bundle_id: &str) -> Option<(&'static str, &'static str)> {
    // Beta/Dev/Canary variantlarni ham qamrash uchun prefix bilan tekshiramiz
    let table: &[(&str, &str, &str)] = &[
        ("com.apple.Safari", "Safari", "current tab"),
        (
            "com.apple.SafariTechnologyPreview",
            "Safari Technology Preview",
            "current tab",
        ),
        ("com.google.Chrome", "Google Chrome", "active tab"),
        ("com.microsoft.edgemac", "Microsoft Edge", "active tab"),
        ("com.brave.Browser", "Brave Browser", "active tab"),
        ("com.vivaldi.Vivaldi", "Vivaldi", "active tab"),
        ("company.thebrowser.Browser", "Arc", "active tab"),
        ("com.operasoftware.Opera", "Opera", "active tab"),
        ("com.operasoftware.OperaGX", "Opera GX", "active tab"),
        ("org.mozilla.firefox", "Firefox", "active tab"),
        ("org.mozilla.firefoxdeveloperedition", "Firefox Developer Edition", "active tab"),
        ("org.mozilla.nightly", "Firefox Nightly", "active tab"),
    ];

    table
        .iter()
        .find(|(prefix, _, _)| bundle_id == *prefix || bundle_id.starts_with(&format!("{prefix}.")))
        .map(|(_, app, term)| (*app, *term))
}

fn fetch_browser_url(bundle_id: &str) -> Option<String> {
    let (_, tab_term) = browser_script_target(bundle_id)?;

    let script_text = format!(
        r#"try
            tell application id "{bid}"
                if not running then return ""
                if (count of windows) is 0 then return ""
                return URL of {term} of front window
            end tell
        on error errMsg
            return "ERR:" & errMsg
        end try"#,
        bid = bundle_id,
        term = tab_term,
    );

    // Use in-process NSAppleScript so the sandbox apple-events exceptions apply.
    // Spawning osascript as a child process does NOT inherit the parent's
    // temporary-exception.apple-events entitlement.
    unsafe {
        use objc2::runtime::{AnyClass, AnyObject};

        let cls = AnyClass::get("NSAppleScript")?;
        let source = objc2_foundation::NSString::from_str(&script_text);

        // +alloc, then -initWithSource: (init consumes the +1 from alloc)
        let raw: *mut AnyObject = objc2::msg_send![cls, alloc];
        let raw: *mut AnyObject = objc2::msg_send![raw, initWithSource: &*source];
        let script: Retained<AnyObject> = Retained::from_raw(raw)?;

        // -executeAndReturnError: — pass a pointer so we can log errors
        let mut error_dict: *mut AnyObject = std::ptr::null_mut();
        let desc: *mut AnyObject =
            objc2::msg_send![&*script, executeAndReturnError: &mut error_dict];

        if !error_dict.is_null() {
            // Convert error dict description to string for logging
            let desc_str: *mut AnyObject = objc2::msg_send![error_dict, description];
            if !desc_str.is_null() {
                let s = &*(desc_str as *const objc2_foundation::NSString);
                eprintln!("[url] NSAppleScript error: {}", s);
            }
        }

        if desc.is_null() {
            return None;
        }

        // -stringValue on NSAppleEventDescriptor gives the result string
        let str_raw: *mut AnyObject = objc2::msg_send![desc, stringValue];
        if str_raw.is_null() {
            return None;
        }
        let ns_str = &*(str_raw as *const objc2_foundation::NSString);
        let url = ns_str.to_string();

        if url.is_empty() || url == "missing value" || url.starts_with("ERR:") {
            if url.starts_with("ERR:") {
                eprintln!("[url] AppleScript returned: {url}");
            }
            None
        } else {
            Some(url)
        }
    }
}
