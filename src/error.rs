#[derive(Debug, Snafu)]
#[snafu(visibility = "pub")]
pub enum ClipboardError {
    #[snafu(display("Could not spawn tokio task, error: {}", source))]
    SpawnBlockingTask { source: tokio::task::JoinError },

    #[cfg(feature = "monitor")]
    #[snafu(display("Could not initialize clipboard, error: {}", source))]
    InitializeX11Clipboard { source: x11_clipboard::error::Error },

    #[cfg(feature = "monitor")]
    #[snafu(display("Could not paste to clipboard, error: {}", source))]
    PasteToX11Clipboard { source: x11_clipboard::error::Error },
}
