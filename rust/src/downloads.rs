use anyhow::Result;
use serde::Serialize;
use std::{path::PathBuf, time::Instant};
use webview2_com::Microsoft::Web::WebView2::Win32::*;

// WebView2 interfaces belong to the UI apartment. Pass only an integer through
// Tao's Send event queue; the interface never leaves this thread-local store.
thread_local! {
    static PENDING: std::cell::RefCell<std::collections::BTreeMap<u64, ICoreWebView2DownloadOperation>> = const { std::cell::RefCell::new(std::collections::BTreeMap::new()) };
    static NEXT_PENDING: std::cell::Cell<u64> = const { std::cell::Cell::new(1) };
}
pub fn enqueue(operation: ICoreWebView2DownloadOperation) -> u64 {
    let id = NEXT_PENDING.with(|next| {
        let id = next.get();
        next.set(id + 1);
        id
    });
    PENDING.with(|pending| pending.borrow_mut().insert(id, operation));
    id
}
pub fn take_pending(id: u64) -> Option<ICoreWebView2DownloadOperation> {
    PENDING.with(|pending| pending.borrow_mut().remove(&id))
}

#[derive(Clone, Serialize)]
pub struct DownloadInfo {
    pub id: u64,
    pub name: String,
    pub received: u64,
    pub total: Option<u64>,
    pub speed: f64,
    pub state: String,
    pub can_resume: bool,
}
pub struct Transfer {
    pub operation: ICoreWebView2DownloadOperation,
    pub info: DownloadInfo,
    pub path: PathBuf,
    pub tab: u64,
    sampled: Instant,
    previous: u64,
}
impl Transfer {
    pub fn new(id: u64, tab: u64, operation: ICoreWebView2DownloadOperation) -> Result<Self> {
        let path = PathBuf::from(crate::engine::string(|p| unsafe {
            operation.ResultFilePath(p)
        })?);
        let mut result = Self {
            operation,
            tab,
            path: path.clone(),
            sampled: Instant::now(),
            previous: 0,
            info: DownloadInfo {
                id,
                name: path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into(),
                received: 0,
                total: None,
                speed: 0.,
                state: "progress".into(),
                can_resume: false,
            },
        };
        result.sample()?;
        Ok(result)
    }
    pub fn sample(&mut self) -> Result<()> {
        let mut bytes = 0;
        let mut total = 0;
        let mut state = COREWEBVIEW2_DOWNLOAD_STATE::default();
        let mut resume = windows::core::BOOL::default();
        unsafe {
            self.operation.BytesReceived(&mut bytes)?;
            self.operation.TotalBytesToReceive(&mut total)?;
            self.operation.State(&mut state)?;
            self.operation.CanResume(&mut resume)?;
        }
        let received = bytes.max(0) as u64;
        let elapsed = self.sampled.elapsed().as_secs_f64();
        if elapsed >= 0.2 {
            self.info.speed = received.saturating_sub(self.previous) as f64 / elapsed;
            self.previous = received;
            self.sampled = Instant::now();
        }
        self.info.received = received;
        self.info.total = (total > 0).then_some(total as u64);
        self.info.state = match state {
            COREWEBVIEW2_DOWNLOAD_STATE_COMPLETED => "completed",
            COREWEBVIEW2_DOWNLOAD_STATE_INTERRUPTED => "interrupted",
            _ => "progress",
        }
        .into();
        self.info.can_resume = resume.as_bool();
        if self.info.state != "progress" {
            self.info.speed = 0.;
        }
        Ok(())
    }
}
