//! WASM trace ring buffer (#159) — a browser flight recorder.
//!
//! `guardrails-trace`'s native JSONL sink can't run in wasm (no fs/stderr), so we
//! hand-roll a minimal `tracing::Subscriber` that captures *events* (not span
//! trees) into a bounded ring buffer, exposed to JS via `__hyrr_dump_trace()`.
//! The events come from the SAME `hyrr_core::trace_schema` constructors used
//! natively, so the dotted `event` names and fields match the JSONL stream and the
//! frontend can merge both. We deliberately avoid `tracing-subscriber` (its
//! `fmt`/`json`/`Registry` machinery is dead weight under `opt-level="s"`).
//!
//! A composed panic hook records the panic into the ring *before* the wasm abort,
//! so `__hyrr_dump_trace()` surfaces the panic that preceded a poisoned-store
//! "unsafe aliasing" failure (#344) — the trail that didn't exist before.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::Serialize;
use serde_json::{Map, Value};
use tracing::field::{Field, Visit};
use tracing::{Event, Metadata, Subscriber};
use wasm_bindgen::prelude::*;

/// Max events retained. ~500 × ~200 B ≈ 100 KB resident — bounded on purpose.
const CAP: usize = 500;

/// One captured event. Field names (`ts`/`src`/`event`/`data`) are the cross-source
/// schema contract shared with the frontend ring buffer and the native JSONL.
#[derive(Serialize, Clone)]
pub struct TraceEvent {
    /// Epoch milliseconds (JS clock). 0.0 on non-wasm test builds.
    pub ts: f64,
    /// Monotonic sequence — stable ordering even when several events share a `ts`.
    pub seq: u64,
    /// Provenance for the merged preview ("wasm" here; the frontend stamps "frontend").
    pub src: &'static str,
    pub level: &'static str,
    /// Dotted event name (the `event = "…"` field), or the target as a fallback.
    pub event: String,
    /// Remaining structured fields.
    pub data: Map<String, Value>,
}

/// Pure bounded ring — no wasm/JS deps, so it is unit-testable on the host.
#[derive(Default)]
pub struct Ring {
    events: VecDeque<TraceEvent>,
}

impl Ring {
    pub fn push(&mut self, ev: TraceEvent) {
        if self.events.len() == CAP {
            self.events.pop_front();
        }
        self.events.push_back(ev);
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.events.len()
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Serialize to `{ "traceId": <id|null>, "events": [...] }` (oldest first).
    pub fn dump_json(&self, trace_id: Option<&str>) -> String {
        let events: Vec<&TraceEvent> = self.events.iter().collect();
        let out = serde_json::json!({ "traceId": trace_id, "events": events });
        serde_json::to_string(&out).unwrap_or_else(|_| "{\"events\":[]}".to_string())
    }

    pub fn clear(&mut self) {
        self.events.clear();
    }
}

thread_local! {
    // wasm is single-threaded, so a thread_local is effectively global. It lives
    // OUTSIDE any `#[wasm_bindgen]` struct's WasmRefCell, so a panic-abort that
    // poisons the data store (#344) cannot poison the trace ring.
    static RING: RefCell<Ring> = RefCell::new(Ring::default());
}

static SEQ: AtomicU64 = AtomicU64::new(0);

fn next_seq() -> u64 {
    SEQ.fetch_add(1, Ordering::Relaxed)
}

#[cfg(target_arch = "wasm32")]
fn now_ms() -> f64 {
    js_sys::Date::now()
}

#[cfg(not(target_arch = "wasm32"))]
fn now_ms() -> f64 {
    0.0
}

/// Collects an event's fields into a JSON map, pulling `event` out as the name.
#[derive(Default)]
struct RingVisitor {
    data: Map<String, Value>,
}

impl Visit for RingVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.data.insert(field.name().to_string(), Value::from(value));
    }
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.data.insert(field.name().to_string(), Value::from(value));
    }
    fn record_u64(&mut self, field: &Field, value: u64) {
        self.data.insert(field.name().to_string(), Value::from(value));
    }
    fn record_f64(&mut self, field: &Field, value: f64) {
        self.data.insert(field.name().to_string(), Value::from(value));
    }
    fn record_bool(&mut self, field: &Field, value: bool) {
        self.data.insert(field.name().to_string(), Value::from(value));
    }
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        // Fallback for `?value` Display/Debug fields. In practice only the
        // already-redacted `%path` strings in trace_schema reach this — never a
        // raw user struct (the no-raw-trace-fields gate enforces that).
        self.data
            .insert(field.name().to_string(), Value::from(format!("{value:?}")));
    }
}

/// Build a `TraceEvent` from a tracing `Event` and push it into the ring.
fn record_event(event: &Event<'_>) {
    let meta = event.metadata();
    let mut visitor = RingVisitor::default();
    event.record(&mut visitor);
    let name = match visitor.data.remove("event") {
        Some(Value::String(s)) => s,
        _ => meta.target().to_string(),
    };
    let ev = TraceEvent {
        ts: now_ms(),
        seq: next_seq(),
        src: "wasm",
        level: meta.level().as_str(),
        event: name,
        data: visitor.data,
    };
    RING.with(|r| r.borrow_mut().push(ev));
}

/// Minimal events-only subscriber. Spans are accepted but not stored — the bug
/// report wants a flight recorder of events, not a profiler (span timings are the
/// native JSONL's job).
struct RingSubscriber;

impl Subscriber for RingSubscriber {
    fn enabled(&self, _meta: &Metadata<'_>) -> bool {
        true
    }
    fn new_span(&self, _span: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }
    fn record(&self, _span: &tracing::span::Id, _values: &tracing::span::Record<'_>) {}
    fn record_follows_from(&self, _span: &tracing::span::Id, _follows: &tracing::span::Id) {}
    fn event(&self, event: &Event<'_>) {
        record_event(event);
    }
    fn enter(&self, _span: &tracing::span::Id) {}
    fn exit(&self, _span: &tracing::span::Id) {}
}

/// Push a synthetic panic event so the trail survives the wasm abort (#344/#159).
fn record_panic(info: &std::panic::PanicHookInfo<'_>) {
    let mut data = Map::new();
    data.insert("message".to_string(), Value::from(format!("{info}")));
    let ev = TraceEvent {
        ts: now_ms(),
        seq: next_seq(),
        src: "wasm",
        level: "ERROR",
        event: "wasm.panic".to_string(),
        data,
    };
    RING.with(|r| r.borrow_mut().push(ev));
}

/// Install the ring subscriber + composed panic hook. Idempotent (first call wins).
/// Call once at engine startup (`WasmDataStore::new`).
pub fn install() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let _ = tracing::subscriber::set_global_default(RingSubscriber);
        // Compose: record into the ring FIRST (survives abort), then the existing
        // console hook so the dev console still shows the panic.
        std::panic::set_hook(Box::new(|info| {
            record_panic(info);
            console_error_panic_hook::hook(info);
        }));
    });
}

/// Dump the ring as `{traceId, events}` JSON. `trace_id` echoes the frontend's id
/// so the merged preview is one correlated trace.
#[wasm_bindgen(js_name = __hyrr_dump_trace)]
pub fn dump_trace(trace_id: Option<String>) -> String {
    RING.with(|r| r.borrow().dump_json(trace_id.as_deref()))
}

/// Clear the ring (e.g. when the frontend starts a fresh session).
#[wasm_bindgen(js_name = __hyrr_clear_trace)]
pub fn clear_trace() {
    RING.with(|r| r.borrow_mut().clear());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(seq: u64) -> TraceEvent {
        TraceEvent {
            ts: 0.0,
            seq,
            src: "wasm",
            level: "INFO",
            event: "test.event".to_string(),
            data: Map::new(),
        }
    }

    #[test]
    fn ring_is_bounded_and_fifo() {
        let mut ring = Ring::default();
        for i in 0..(CAP as u64 + 100) {
            ring.push(ev(i));
        }
        assert_eq!(ring.len(), CAP, "ring must stay bounded at CAP");
        // Oldest 100 evicted: first retained seq is 100.
        let json = ring.dump_json(Some("abc"));
        assert!(json.contains("\"traceId\":\"abc\""));
        assert!(json.contains("\"seq\":100"), "oldest entries must be evicted");
        assert!(!json.contains("\"seq\":99"), "evicted entry must be gone");
    }

    #[test]
    fn dump_shape_has_trace_id_and_events() {
        let mut ring = Ring::default();
        ring.push(ev(0));
        let json = ring.dump_json(None);
        assert!(json.contains("\"traceId\":null"));
        assert!(json.contains("\"events\":["));
        assert!(json.contains("\"src\":\"wasm\""));
        assert!(json.contains("\"event\":\"test.event\""));
    }

    #[test]
    fn clear_empties_the_ring() {
        let mut ring = Ring::default();
        ring.push(ev(0));
        assert!(!ring.is_empty());
        ring.clear();
        assert!(ring.is_empty());
    }
}
