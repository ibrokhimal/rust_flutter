use crate::api::types::WindowInfo;
use crate::frb_generated::StreamSink;
use crate::platform;
use std::time::Duration;

/// Hozirgi active window — bir martalik so'rov
pub async fn current_window() -> anyhow::Result<Option<WindowInfo>> {
    platform::current_window().await
}

/// Active window'ni uzluksiz kuzatish.
///
/// Gibrid yondashuv:
///   - Har 50 ms da faqat `frontmost_id()` ni tekshiradi (mikrosekund, arzon).
///   - ID o'zgarsa — darhol to'liq ma'lumot oladi (app switch ~50 ms ichida seziladi).
///   - Har `poll_ms` ms da to'liq tekshiruv ham bajariladi (title/URL o'zgarishi uchun).
pub async fn watch_active_window(
    sink: StreamSink<WindowInfo>,
    poll_ms: u32,
) -> anyhow::Result<()> {
    const FAST_MS: u64 = 50;
    let full_ms = poll_ms.max(100) as u64;
    // How many fast ticks before forcing a full check (for title/URL changes)
    let full_every = (full_ms / FAST_MS).max(1);

    let mut last: Option<WindowInfo> = None;
    let mut last_id: u64 = 0;
    let mut tick: u64 = 0;

    loop {
        tick += 1;
        let current_id = platform::frontmost_id();

        // Collect if: app/window switched  OR  periodic full check
        if current_id != last_id || tick % full_every == 0 {
            last_id = current_id;

            match platform::current_window().await {
                Ok(Some(info)) => {
                    if last.as_ref() != Some(&info) {
                        if sink.add(info.clone()).is_err() {
                            break;
                        }
                        last = Some(info);
                    }
                }
                Ok(None) => {}
                Err(e) => eprintln!("monitor error: {e:?}"),
            }
        }

        tokio::time::sleep(Duration::from_millis(FAST_MS)).await;
    }
    Ok(())
}
