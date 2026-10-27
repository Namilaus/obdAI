use super::*;
use std::{collections::BTreeMap, sync::Mutex};

#[derive(Default)]
pub struct MeasurementCache { state: Mutex<(u64, BTreeMap<String, Measurement>)> }
impl MeasurementCache {
    pub fn reset(&self, generation: u64) { *self.state.lock().unwrap() = (generation, BTreeMap::new()); }
    pub fn publish(&self, mut measurement: Measurement) -> Result<(), RecordingError> {
        let mut state = self.state.lock().unwrap();
        if measurement.name.chars().count()>256 || measurement.metric_id.len()>4096 || measurement.unit.len()>128 || (!state.1.contains_key(&measurement.metric_id) && state.1.len()>=256) { return Err(RecordingError::new("invalid_input","Recording metric limits exceeded.")); }
        if state.0 != measurement.generation { return Err(RecordingError::new("stale_connection", "This reading belongs to an earlier connection.")); }
        if measurement.value.map(|v| !v.is_finite()).unwrap_or(false) {
            measurement.value = None; measurement.availability = Availability::Error;
        }
        if measurement.availability != Availability::Valid { measurement.value = None; }
        if state.1.get(&measurement.metric_id).map(|old| old.observation_id >= measurement.observation_id).unwrap_or(false) { return Ok(()); }
        state.1.insert(measurement.metric_id.clone(), measurement);
        Ok(())
    }
    pub fn snapshot(&self, generation: u64) -> Vec<Measurement> {
        let state = self.state.lock().unwrap();
        if state.0 != generation { return Vec::new(); }
        state.1.values().cloned().collect()
    }
}
