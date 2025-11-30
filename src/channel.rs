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
#[cfg(test)]
mod tests {
    use super::{Event, mpsc_channel, read_event};
    use serde_json::json;

    #[tokio::test]
    async fn channel() {
        let (tx, mut rx) = mpsc_channel();

        let manager = tokio::spawn(async move {
            let mut results: Vec<Event> = Vec::new();
            while let Some(event) = read_event(&mut rx).await {
                match event {
                    Event::Terminate => break,
                    _ => {
                        results.push(event);
                    }
                }
            }
            return results;
        });

        tx.send(Event::Login(vec![json!("{}")]))
            .await
            .expect("Failed to send event");
        tx.send(Event::Chat(vec![json!("{\"time\": 123456789}")]))
            .await
            .expect("Failed to send event");
        tx.send(Event::Terminate)
            .await
            .expect("Failed to send event");

        let results = manager.await.unwrap();
        assert!(matches!(results[0], Event::Login(_)));
        assert!(matches!(results[1], Event::Chat(_)));
    }
}
