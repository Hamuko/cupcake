use crate::Event;

const MESSAGE_BUFFER_SIZE: usize = 64;

#[cfg(feature = "tokio_channels")]
pub type EventTx = tokio::sync::mpsc::Sender<Event>;
#[cfg(feature = "tokio_channels")]
pub type EventRx = tokio::sync::mpsc::Receiver<Event>;

#[cfg(feature = "crossfire_channels")]
pub type EventTx = crossfire::MAsyncTx<Event>;
#[cfg(feature = "crossfire_channels")]
pub type EventRx = crossfire::AsyncRx<Event>;

#[cfg(feature = "crossfire_channels")]
pub fn mpsc_channel() -> (EventTx, EventRx) {
    log::debug!(
        "Creating crossfire channel with buffer size {}",
        MESSAGE_BUFFER_SIZE
    );
    crossfire::mpsc::bounded_async(MESSAGE_BUFFER_SIZE)
}

#[cfg(feature = "tokio_channels")]
pub fn mpsc_channel() -> (EventTx, EventRx) {
    log::debug!(
        "Creating tokio sync channel with buffer size {}",
        MESSAGE_BUFFER_SIZE
    );
    tokio::sync::mpsc::channel(MESSAGE_BUFFER_SIZE)
}

#[cfg(feature = "crossfire_channels")]
pub async fn read_event(rx: &mut EventRx) -> Option<Event> {
    rx.recv().await.ok()
}

#[cfg(feature = "tokio_channels")]
pub async fn read_event(rx: &mut EventRx) -> Option<Event> {
    rx.recv().await
}
