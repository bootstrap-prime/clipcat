#![allow(unused_imports)]
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
};

use copypasta::{ClipboardContext, ClipboardProvider};
use snafu::ResultExt;
use tokio::sync::broadcast::{self, error::SendError};

use crate::{error, ClipboardError, ClipboardEvent, ClipboardType, MonitorState};

pub struct ClipboardMonitor {
    is_running: Arc<AtomicBool>,
    event_sender: broadcast::Sender<ClipboardEvent>,
    clipboard_thread: Option<thread::JoinHandle<()>>,
    primary_thread: Option<thread::JoinHandle<()>>,
}

#[derive(Debug, Clone, Copy)]
pub struct ClipboardMonitorOptions {
    pub load_current: bool,
    pub enable_clipboard: bool,
    pub enable_primary: bool,
    pub filter_min_size: usize,
}

impl Default for ClipboardMonitorOptions {
    fn default() -> Self {
        ClipboardMonitorOptions {
            load_current: true,
            enable_clipboard: true,
            enable_primary: true,
            filter_min_size: 0,
        }
    }
}

impl ClipboardMonitor {
    pub fn new(opts: ClipboardMonitorOptions) -> Result<ClipboardMonitor, ClipboardError> {
        let (event_sender, _event_receiver) = broadcast::channel(16);

        let is_running = Arc::new(AtomicBool::new(true));
        let mut monitor = ClipboardMonitor {
            is_running: is_running.clone(),
            event_sender: event_sender.clone(),
            clipboard_thread: None,
            primary_thread: None,
        };

        if opts.enable_clipboard {
            let thread = build_thread(
                opts.load_current,
                is_running.clone(),
                ClipboardType::Clipboard,
                event_sender.clone(),
                opts.filter_min_size,
            )?;
            monitor.clipboard_thread = Some(thread);
        }

        if opts.enable_primary {
            let thread = build_thread(
                opts.load_current,
                is_running,
                ClipboardType::Primary,
                event_sender,
                opts.filter_min_size,
            )?;
            monitor.primary_thread = Some(thread);
        }

        if monitor.clipboard_thread.is_none() && monitor.primary_thread.is_none() {
            tracing::warn!("Both clipboard and primary are not monitored");
        }

        Ok(monitor)
    }

    #[inline]
    pub fn subscribe(&self) -> broadcast::Receiver<ClipboardEvent> { self.event_sender.subscribe() }

    #[inline]
    pub fn enable(&mut self) {
        self.is_running.store(true, Ordering::Release);
        tracing::info!("ClipboardWorker is monitoring for clipboard");
    }

    #[inline]
    pub fn disable(&mut self) {
        self.is_running.store(false, Ordering::Release);
        tracing::info!("ClipboardWorker is not monitoring for clipboard");
    }

    #[inline]
    pub fn toggle(&mut self) {
        if self.is_running() {
            self.disable();
        } else {
            self.enable();
        }
    }

    #[inline]
    pub fn is_running(&self) -> bool { self.is_running.load(Ordering::Acquire) }

    #[inline]
    pub fn state(&self) -> MonitorState {
        if self.is_running() {
            MonitorState::Enabled
        } else {
            MonitorState::Disabled
        }
    }
}

fn build_thread(
    load_current: bool,
    is_running: Arc<AtomicBool>,
    clipboard_type: ClipboardType,
    sender: broadcast::Sender<ClipboardEvent>,
    filter_min_size: usize,
) -> Result<thread::JoinHandle<()>, ClipboardError> {
    let get_clipboard = || match clipboard_type {
        ClipboardType::Clipboard => ClipboardContext::new(),
        ClipboardType::Primary => {
            #[cfg(feature = "wayland")]
            return Err("Primary clipboard integration not supported on wayland.");

            #[cfg(feature = "x11")]
            use copypasta::x11_clipboard::{Primary, X11ClipboardContext};
            #[cfg(feature = "x11")]
            X11ClipboardContext::<Primary>::new()
        }
    };

    let send_event = move |data: &str| {
        let event = match clipboard_type {
            ClipboardType::Clipboard => ClipboardEvent::new_clipboard(data),
            ClipboardType::Primary => ClipboardEvent::new_primary(data),
        };
        sender.send(event)
    };

    let clipboard: Box<dyn copypasta::ClipboardProvider> =
        get_clipboard.context(error::InitializeX11Clipboard)?;

    let join_handle = thread::spawn(move || {
        let mut clipboard = clipboard;

        let mut last = if load_current {
            let result = clipboard.load();
            match result {
                Ok(data) => {
                    if data.len() > filter_min_size {
                        if let Err(SendError(_curr)) = send_event(&data) {
                            tracing::info!("ClipboardEvent receiver is closed.");
                            return;
                        }
                    }
                    data
                }
                Err(_) => String::new(),
            }
        } else {
            String::new()
        };

        loop {
            let result = clipboard.load_wait();
            match result {
                Ok(curr) => {
                    if is_running.load(Ordering::Acquire)
                        && curr.len() > filter_min_size
                        && last.as_bytes() != curr.as_bytes()
                    {
                        if let Err(SendError(_curr)) = send_event(&last) {
                            tracing::info!("ClipboardEvent receiver is closed.");
                            return;
                        };
                    }
                }
                Err(err) => {
                    tracing::error!(
                        "Failed to load clipboard, error: {}. Restarting clipboard provider.",
                        err,
                    );
                    clipboard = match get_clipboard {
                        Ok(c) => c,
                        Err(err) => {
                            tracing::error!("Failed to restart clipboard provider, error: {}", err);
                            std::process::exit(1)
                        }
                    }
                }
            }
        }
    });

    Ok(join_handle)
}

// type ClipResult<T> = std::result::Result<T, Box<dyn std::error::Error + Send
// + Sync + 'static>>;

// struct ClipboardWaitProvider {
//     clipboard_type: ClipboardType,
//     clipboard: Box<dyn copypasta::ClipboardProvider>,
// }

// impl ClipboardWaitProvider {
//     pub(crate) fn new(clipboard_type: ClipboardType) -> ClipResult<Self> {
//         Ok(Self { clipboard, clipboard_type })
//     }

//     pub(crate) fn load(&self) -> ClipResult<String> {
// self.clipboard.get_contents() }

//     pub(crate) fn load_wait(&self) -> ClipResult<String> {
//         loop {
//             let response = self.clipboard.get_contents()?;
//             match response {
//                 contents if !contents.is_empty() => {
//                     return Ok(contents);
//                 }
//                 _ => {
//                     thread::sleep(std::time::Duration::from_millis(250));
//                 }
//             }
//         }
//     }
// }
