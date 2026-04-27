use crate::api::types::WindowInfo;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

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
