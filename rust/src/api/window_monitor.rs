use crate::api::types::WindowInfo;
use crate::frb_generated::StreamSink;
use crate::platform;
use std::time::Duration;

/// Hozirgi active window — bir martalik so'rov
pub async fn current_window() -> anyhow::Result<Option<WindowInfo>> {
    platform::current_window().await
}

/// Active window'ni uzluksiz kuzatish.
pub async fn watch_active_window(
    sink: StreamSink<WindowInfo>,
    poll_ms: u32, // u64 emas
) -> anyhow::Result<()> {
    let interval = Duration::from_millis(poll_ms.max(100) as u64);
    let mut last: Option<WindowInfo> = None;

    loop {
        match platform::current_window().await {
            Ok(Some(info)) => {
                // faqat o'zgarganda yuboramiz — Flutter keraksiz rebuild qilmasligi uchun
                if last.as_ref() != Some(&info) {
                    if sink.add(info.clone()).is_err() {
                        // Flutter listener'ni yopib qo'ydi — chiqamiz
                        break;
                    }
                    last = Some(info);
                }
            }
            Ok(None) => {} // hech qaysi window focus'da emas
            Err(e) => eprintln!("monitor error: {e:?}"),
        }
        tokio::time::sleep(interval).await;
    }
    Ok(())
}
