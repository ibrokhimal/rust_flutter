// icon_png excluded from PartialEq: it never changes for a given process_path,
// so comparing kilobytes of PNG bytes every 400 ms is pure waste.
#[derive(Debug, Clone)]
pub struct WindowInfo {
    pub title: String,
    pub process_name: String,
    pub process_path: String,
    pub icon_png: Option<Vec<u8>>,
    pub url: Option<String>,
}

impl PartialEq for WindowInfo {
    fn eq(&self, other: &Self) -> bool {
        self.title == other.title
            && self.process_name == other.process_name
            && self.process_path == other.process_path
            && self.url == other.url
    }
}

impl Eq for WindowInfo {}

#[derive(Debug, thiserror::Error)]
pub enum MonitorError {
    #[error("Platforma qo'llab-quvvatlanmaydi")]
    Unsupported,
    #[error("Tizim xatosi: {0}")]
    Os(String),
}