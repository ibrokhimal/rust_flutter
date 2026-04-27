use crate::api::types::WindowInfo;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

/// Cheap check: returns an opaque ID that changes whenever the frontmost
/// process/window changes.  Used to detect app switches without doing the
/// expensive full collection (title, icon, URL).
pub fn frontmost_id() -> u64 {
    #[cfg(target_os = "macos")]
    {
        macos::frontmost_pid() as u64
    }
    #[cfg(target_os = "windows")]
    {
        windows::frontmost_hwnd()
    }
    #[cfg(target_os = "linux")]
    {
        linux::frontmost_xid() as u64
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        0
    }
}

pub async fn current_window() -> anyhow::Result<Option<WindowInfo>> {
    #[cfg(target_os = "windows")]
    {
        return windows::current_window().await;
    }

    #[cfg(target_os = "macos")]
    {
        return macos::current_window().await;
    }

    #[cfg(target_os = "linux")]
    {
        return linux::current_window().await;
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        Err(anyhow::anyhow!("Unsupported platform"))
    }
}
