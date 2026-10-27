mod cache;
mod queries;
mod recorder;
mod store;
mod types;
pub use cache::*;
pub use recorder::*;
pub use store::*;
pub use types::*;

pub static ACTIVE_RECORDER: std::sync::OnceLock<RecorderHandle> = std::sync::OnceLock::new();

/// In-memory only; safe to call while scanner access is locked.
pub fn publish_scalar(
    name: &str,
    scalar: &crate::scalar::Scalar,
    generation: u64,
    definition: Option<&str>,
) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT_OBSERVATION: AtomicU64 = AtomicU64::new(1);
    let Some(handle) = ACTIVE_RECORDER.get() else {
        return;
    };
    let id = if let Some(definition) = definition {
        format!("custom:{definition}")
    } else {
        format!("builtin:{name}")
    };
    let _ = handle.cache.publish(Measurement {
        metric_id: id,
        name: name.into(),
        observation_id: NEXT_OBSERVATION.fetch_add(1, Ordering::Relaxed),
        generation,
        acquired_at: scalar.acquired_at.clone(),
        value: if scalar.unit == crate::scalar::Unit::NoData {
            None
        } else {
            Some(scalar.value as f64)
        },
        unit: scalar.unit.as_str().into(),
        availability: if scalar.unit == crate::scalar::Unit::NoData {
            Availability::NoData
        } else {
            Availability::Valid
        },
    });
}
