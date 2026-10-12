//! Compatibility re-export of provider-neutral observations.
pub use aillies_core::telemetry::*;
#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::{stream, StreamExt};
    use std::sync::{Arc, Mutex};

    #[tokio::test]
    async fn terminal_failure_and_cancellation_are_observed_once_without_content() {
        for (chunks, expected) in [
            (vec!["event: response.completed\ndata: {}\n\n"], "completed"),
            (vec!["event: response.failed\ndata: {}\n\n"], "failed"),
            (vec!["data: {\"text\":\"SECRET_OUTPUT response.completed\"}\n\n"], "delivery_uncertain"),
        ] {
            let events = Arc::new(Mutex::new(Vec::new()));
            let copy = events.clone();
            let mut guard = Observation::start(Some(Arc::new(move |e| copy.lock().unwrap().push(e))));
            guard.headers(200);
            let input: TurnStream = Box::pin(stream::iter(chunks.into_iter().map(|s| Ok(bytes::Bytes::from(s)))));
            observe(input, guard).collect::<Vec<_>>().await;
            let serialized = serde_json::to_value(&*events.lock().unwrap()).unwrap();
            assert_eq!(serialized.as_array().unwrap().last().unwrap()["phase"], expected);
            assert_eq!(serialized.as_array().unwrap().len(), 4);
            assert!(!serialized.to_string().contains("SECRET"));
        }
        let events = Arc::new(Mutex::new(Vec::new()));
        let copy = events.clone();
        let guard = Observation::start(Some(Arc::new(move |e| copy.lock().unwrap().push(e))));
        drop(observe(Box::pin(stream::pending()), guard));
        let serialized = serde_json::to_value(&*events.lock().unwrap()).unwrap();
        assert_eq!(serialized[1]["phase"], "delivery_uncertain");
    }
}
