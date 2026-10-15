//! Typed upstream observations. This interface cannot carry request/response content.
use crate::Failure;
use bytes::Bytes;
use futures_util::Stream;
use std::pin::Pin;
pub type TurnStream = Pin<Box<dyn Stream<Item = Result<Bytes, Failure>> + Send>>;
use crate::{wire::TerminalScanner, FailureCode};
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
#[serde(rename_all = "snake_case")]
pub enum Effort {
    None,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
    Max,
    Ultra,
    Other,
    Unspecified,
}

#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpstreamEvent {
    pub phase: Phase,
    pub status: Option<u16>,
    pub elapsed_ms: u64,
    pub first_byte_ms: Option<u64>,
    pub failure: Option<FailureCode>,
    /// Actual prepared Claude request metadata. None means not observed, including old logs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_reasoning_effort: Option<Effort>,
}
pub type Observer = Arc<dyn Fn(UpstreamEvent) + Send + Sync>;

pub struct Observation {
    observer: Option<Observer>,
    started: Instant,
    status: Option<u16>,
    first_byte_ms: Option<u64>,
    finished: bool,
    effective_reasoning_effort: Option<Effort>,
}
impl Observation {
    pub fn start(observer: Option<Observer>) -> Self {
        let this = Self { observer, started: Instant::now(), status: None, first_byte_ms: None, finished: false, effective_reasoning_effort: None };
        this.emit(Phase::Sending, None);
        this
    }
    fn elapsed(&self) -> u64 {
        self.started.elapsed().as_millis().min(u64::MAX as u128) as u64
    }
    fn emit(&self, phase: Phase, failure: Option<FailureCode>) {
        if let Some(observer) = &self.observer {
            observer(UpstreamEvent {
                phase,
                status: self.status,
                elapsed_ms: self.elapsed(),
                first_byte_ms: self.first_byte_ms,
                failure,
                effective_reasoning_effort: self.effective_reasoning_effort,
            });
        }
    }
    pub fn prepared_effort(&mut self, effort: Effort) {
        self.effective_reasoning_effort = Some(effort);
        self.emit(Phase::Sending, None);
    }
    pub fn headers(&mut self, status: u16) {
        self.status = Some(status);
        self.emit(Phase::HeadersReceived, None);
    }
    pub fn bytes(&mut self) {
        if self.first_byte_ms.is_none() {
            self.first_byte_ms = Some(self.elapsed());
            self.emit(Phase::Receiving, None);
        }
    }
    pub fn finish(&mut self, failure: Option<FailureCode>) {
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

pub fn observe(input: TurnStream, guard: Observation) -> TurnStream {
    observe_with(input, guard, TerminalScanner::default())
}

pub fn observe_with(input: TurnStream, guard: Observation, scanner: TerminalScanner) -> TurnStream {
    Box::pin(stream::unfold((input, guard, scanner), |(mut input, mut guard, mut scanner)| async move {
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
