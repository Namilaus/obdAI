use super::*;
use sqlite::State;
use std::collections::BTreeMap;
use crate::scalar::{Scalar,Unit};

fn normalized(value:f64,unit:&str)->(f64,String) {
    let parsed=unit.parse::<Unit>().ok();
    let target=match parsed {Some(Unit::Fahrenheit)=>Some(Unit::Celsius),Some(Unit::MilesPerHour)=>Some(Unit::KilometersPerHour),Some(Unit::Miles|Unit::Meters|Unit::Feet)=>Some(Unit::Kilometers),Some(Unit::PSI|Unit::Pascal)=>Some(Unit::KiloPascal),Some(Unit::FootPounds)=>Some(Unit::NewtonMeters),Some(Unit::GallonsPerHour)=>Some(Unit::LitresPerHour),_=>None};
    if let (Some(from),Some(to))=(parsed,target) { if let Some(s)=Scalar::new(value as f32,from,None).convert(to) { return (s.value as f64,s.unit.as_str().into()); } }
    (value,unit.into())
}

impl RecordingStore {
    pub fn summary(&self,id:&str,metrics:&[String],start:u64,end:u64)->Result<Vec<MetricSummary>,RecordingError> {
        self.summary_cancellable(id,metrics,start,end,||false)
    }
    pub fn summary_cancellable(&self,id:&str,metrics:&[String],start:u64,end:u64,cancelled:impl Fn()->bool)->Result<Vec<MetricSummary>,RecordingError> {
        self.session(id)?;
        if start>end || metrics.len()>10 { return Err(RecordingError::new("invalid_input","Invalid summary selection.")); }
        let mut s=self.connection.prepare("SELECT payload FROM measurements WHERE session_id=? AND elapsed_ms>=? AND elapsed_ms<? ORDER BY id")?;
        s.bind((1,id))?;s.bind((2,start.min(i64::MAX as u64) as i64))?;s.bind((3,end.min(i64::MAX as u64) as i64))?;
        let mut summaries:BTreeMap<(String,String),(MetricSummary,Option<u64>)>=BTreeMap::new();
        while s.next()?==State::Row {
            if cancelled(){return Err(RecordingError::new("cancelled","Recording query cancelled."));}
            let row:SampleRow=serde_json::from_str(&s.read::<String,_>(0)?)?;
            let m=&row.measurement;
            if !metrics.is_empty() && !metrics.contains(&m.metric_id) {continue;}
            let (value,unit)=normalized(m.value.unwrap_or(0.0),&m.unit);
            let entry=summaries.entry((m.metric_id.clone(),unit.clone())).or_insert_with(||(MetricSummary{metric_id:m.metric_id.clone(),name:m.name.clone(),unit,min:None,max:None,mean:None,first:None,last:None,first_acquired_at:None,last_acquired_at:None,unique_observations:0,repeated_snapshots:0,stale_snapshots:0,unavailable_snapshots:0},None));
            let (summary,last_id)=entry;
            if row.repeated {summary.repeated_snapshots+=1;}
            if row.stale {summary.stale_snapshots+=1;}
            if m.availability!=Availability::Valid || m.value.is_none() {summary.unavailable_snapshots+=1;continue;}
            if row.stale || *last_id==Some(m.observation_id) {continue;}
            *last_id=Some(m.observation_id);summary.unique_observations+=1;
            summary.min=Some(summary.min.map(|v|v.min(value)).unwrap_or(value));summary.max=Some(summary.max.map(|v|v.max(value)).unwrap_or(value));
            summary.mean=Some(summary.mean.unwrap_or(0.0)+(value-summary.mean.unwrap_or(0.0))/summary.unique_observations as f64);
            if summary.first.is_none(){summary.first=Some(value);summary.first_acquired_at=Some(m.acquired_at.clone());}
            summary.last=Some(value);summary.last_acquired_at=Some(m.acquired_at.clone());
        }
        Ok(summaries.into_values().map(|v|v.0).collect())
    }
    pub fn metrics(&self,id:&str)->Result<Vec<Measurement>,RecordingError> {
        self.session(id)?;
        let mut s=self.connection.prepare("SELECT payload FROM measurements WHERE id IN (SELECT MAX(id) FROM measurements WHERE session_id=? GROUP BY metric_id) ORDER BY metric_id")?;
        s.bind((1,id))?;let mut out=Vec::new();while s.next()?==State::Row {let row:SampleRow=serde_json::from_str(&s.read::<String,_>(0)?)?;out.push(row.measurement);}
        Ok(out)
    }
    pub fn diagnostic_context(&self,id:&str)->Result<serde_json::Value,RecordingError> {
        self.session(id)?;
        let mut s=self.connection.prepare("SELECT kind,acquired_utc,payload FROM diagnostic_events WHERE id IN (SELECT MAX(id) FROM diagnostic_events WHERE session_id=? GROUP BY kind)")?;s.bind((1,id))?;
        let mut out=serde_json::json!({"vehicle":{"status":"not_available"},"trouble_codes":{"status":"not_scanned"},"readiness":{"status":"not_scanned"}});
        while s.next()?==State::Row {let kind=s.read::<String,_>(0)?;out[kind]=serde_json::json!({"acquiredUtcMs":s.read::<i64,_>(1)?,"data":serde_json::from_str::<serde_json::Value>(&s.read::<String,_>(2)?)?});}
        Ok(out)
    }
    pub fn assign_vehicle(&mut self,id:&str,vehicle_id:&str,label:&str)->Result<SessionMetadata,RecordingError> {
        if uuid::Uuid::parse_str(vehicle_id).is_err() || label.chars().count()>256 {return Err(RecordingError::new("invalid_input","Choose a vehicle identity and a label up to 256 characters."));}
        let mut session=self.session(id)?;
        if session.ended_at.is_none(){return Err(RecordingError::new("busy","Stop the recording before changing its vehicle."));}
        let mut exists=self.connection.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='conversation_recordings'")?;
        if exists.next()?==State::Row {
            let mut s=self.connection.prepare("SELECT 1 FROM conversation_recordings WHERE session_id=? LIMIT 1")?;s.bind((1,id))?;
            if s.next()?==State::Row {return Err(RecordingError::new("busy","Delete linked conversations before changing this recording's vehicle."));}
        }
        session.vehicle_id=vehicle_id.into();session.label=label.into();self.save_metadata(&session)?;Ok(session)
    }
    pub fn delete_preview(&self,id:&str)->Result<DeletionPreview,RecordingError> {
        self.session(id)?;
        let mut s=self.connection.prepare("SELECT revision FROM recording_sessions WHERE id=?")?;s.bind((1,id))?;s.next()?;let revision=s.read::<i64,_>(0)?;
        let mut ids=Vec::new();let mut revisions=Vec::new();
        let mut exists=self.connection.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='conversation_recordings'")?;
        if exists.next()?==State::Row {
            let mut s=self.connection.prepare("SELECT c.id,c.revision FROM conversations c JOIN conversation_recordings r ON r.conversation_id=c.id WHERE r.session_id=? ORDER BY c.id")?;s.bind((1,id))?;
            while s.next()?==State::Row {ids.push(s.read::<String,_>(0)?);revisions.push(s.read::<i64,_>(1)?);}
        }
        Ok(DeletionPreview{recording_id:id.into(),revision_token:format!("{id}:{revision}:{}:{revisions:?}",ids.join(",")),conversation_ids:ids})
    }
    pub fn delete_recording(&mut self,id:&str,token:&str)->Result<(),RecordingError> {
        if self.session(id)?.ended_at.is_none(){return Err(RecordingError::new("busy","Stop this recording before deleting it."));}
        self.connection.execute("BEGIN IMMEDIATE")?;
        let result=(||->Result<(),RecordingError>{
            let preview=self.delete_preview(id)?;
            if preview.revision_token!=token{return Err(RecordingError::new("changed","Recording or linked chats changed. Review the deletion again."));}
            for conversation in preview.conversation_ids {let mut s=self.connection.prepare("DELETE FROM conversations WHERE id=?")?;s.bind((1,conversation.as_str()))?;s.next()?;}
            let mut s=self.connection.prepare("DELETE FROM recording_sessions WHERE id=?")?;s.bind((1,id))?;s.next()?;Ok(())
        })();
        match result{Ok(())=>{self.connection.execute("COMMIT")?;Ok(())},Err(e)=>{let _=self.connection.execute("ROLLBACK");Err(e)}}
    }
}
