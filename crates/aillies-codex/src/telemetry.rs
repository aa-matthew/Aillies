//! Typed upstream observations. This interface cannot carry request/response content.
use crate::TurnStream;
use aillies_core::{wire::TerminalScanner, FailureCode};
use futures_util::{stream, StreamExt};
use std::{sync::Arc, time::Instant};

#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Received,
    Sending,
    HeadersReceived,
    Receiving,
    Completed,
    Failed,
    DeliveryUncertain,
}

#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpstreamEvent {
    pub phase: Phase,
    pub status: Option<u16>,
    pub elapsed_ms: u64,
    pub first_byte_ms: Option<u64>,
    pub failure: Option<FailureCode>,
}
pub type Observer = Arc<dyn Fn(UpstreamEvent) + Send + Sync>;

pub(crate) struct Observation {
    observer: Option<Observer>,
    started: Instant,
    status: Option<u16>,
    first_byte_ms: Option<u64>,
    finished: bool,
}
impl Observation {
    pub(crate) fn start(observer: Option<Observer>) -> Self {
        let this = Self { observer, started: Instant::now(), status: None, first_byte_ms: None, finished: false };
        this.emit(Phase::Sending, None);
        this
    }
    fn elapsed(&self) -> u64 {
        self.started.elapsed().as_millis().min(u64::MAX as u128) as u64
    }
    fn emit(&self, phase: Phase, failure: Option<FailureCode>) {
        if let Some(observer) = &self.observer {
            observer(UpstreamEvent { phase, status: self.status, elapsed_ms: self.elapsed(), first_byte_ms: self.first_byte_ms, failure });
        }
    }
    pub(crate) fn headers(&mut self, status: u16) {
        self.status = Some(status);
        self.emit(Phase::HeadersReceived, None);
    }
    pub(crate) fn bytes(&mut self) {
        if self.first_byte_ms.is_none() {
            self.first_byte_ms = Some(self.elapsed());
            self.emit(Phase::Receiving, None);
        }
    }
    pub(crate) fn finish(&mut self, failure: Option<FailureCode>) {
        if self.finished {
            return;
        }
        self.finished = true;
        let phase = match failure {
            None => Phase::Completed,
            Some(FailureCode::DeliveryUncertain) => Phase::DeliveryUncertain,
            Some(_) => Phase::Failed,
        };
        self.emit(phase, failure);
    }
}
impl Drop for Observation {
    fn drop(&mut self) {
        self.finish(Some(FailureCode::DeliveryUncertain));
    }
}

pub(crate) fn observe(input: TurnStream, guard: Observation) -> TurnStream {
    Box::pin(stream::unfold((input, guard, TerminalScanner::default()), |(mut input, mut guard, mut scanner)| async move {
        match input.next().await {
            Some(item) => {
                match &item {
                    Ok(bytes) => {
                        guard.bytes();
                        scanner.feed(bytes);
                        if scanner.unsuccessful {
                            guard.finish(Some(FailureCode::StreamFailed));
                        } else if scanner.completed {
                            guard.finish(None);
                        }
                    }
                    Err(f) => guard.finish(Some(f.code)),
                }
                Some((item, (input, guard, scanner)))
            }
            None => {
                guard.finish(if scanner.completed && !scanner.unsuccessful { None } else { Some(FailureCode::DeliveryUncertain) });
                None
            }
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

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
