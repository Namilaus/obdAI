use super::*;
use std::{path::Path, sync::{Arc, Mutex, mpsc}, time::Duration};

/// Single-threaded state machine. Tests supply timestamps; the worker supplies a monotonic clock.
pub struct Recorder {
    pub store: RecordingStore,
    pub cache: Arc<MeasurementCache>,
    connection: Option<ConnectionContext>,
    active: Option<SessionMetadata>,
    next_tick: u64,
    status: RecordingStatus,
    diagnostic_events: std::collections::BTreeMap<String,(Timestamp,serde_json::Value)>,
}
impl Recorder {
    pub fn new(mut store: RecordingStore, cache: Arc<MeasurementCache>) -> Self {
        let recovery = (|| -> Result<(), RecordingError> {
            let mut offset=0;
            loop {
                let sessions=store.list_sessions(100,offset)?;
                if sessions.is_empty() { break; }
                for session in &sessions { if session.ended_at.is_none() { store.finalize(&session.id,"interrupted",Timestamp::now())?; } }
                offset+=sessions.len();
            }
            Ok(())
        })();
        Self { store, cache, connection:None, active:None, next_tick:0, diagnostic_events:Default::default(),
            status:RecordingStatus { error:recovery.err().map(|e|e.message), ..Default::default() } }
    }
    pub fn set_connection(&mut self, context: Option<ConnectionContext>) {
        let changed=match (&self.connection,&context) { (Some(a),Some(b))=>a.generation!=b.generation || a.source!=b.source, (None,None)=>false, _=>true };
        if changed && self.active.is_some() { let _=self.stop(if context.is_none(){"disconnect"}else{"source_changed"},Timestamp::now()); }
        if changed { self.cache.reset(context.as_ref().map(|c|c.generation).unwrap_or(0)); self.diagnostic_events.clear(); }
        self.connection=context;
    }
    pub fn start(&mut self, interval: u64, label: String, now: Timestamp) -> Result<SessionMetadata,RecordingError> {
        if self.active.is_some() { return Err(RecordingError::new("busy","A recording is already running.")); }
        if ![1,2,5,10].contains(&interval) || label.chars().count()>256 { return Err(RecordingError::new("invalid_input","Choose a 1, 2, 5 or 10 second interval and a label up to 256 characters.")); }
        let context=self.connection.clone().ok_or_else(||RecordingError::new("not_connected","Connect a scanner before starting a recording."))?;
        let mut session=SessionMetadata::new(context.vehicle_id,label,interval,context.source,context.generation,now.clone());
        session.simulated=context.simulated;
        self.store.insert_session(&session)?;
        for (kind,(timestamp,payload)) in &self.diagnostic_events { self.store.record_diagnostic(&session.id,kind,timestamp.utc_ms,payload)?; }
        self.active=Some(session.clone()); self.next_tick=now.elapsed_ms+interval*1000;
        self.status=RecordingStatus { recording:true,active_session:Some(session.id.clone()),snapshot_count:0,error:None };
        if let Err(e)=self.capture(now) { self.fail(&e); return Err(e); }
        self.store.session(&session.id)
    }
    fn capture(&mut self, now: Timestamp) -> Result<(),RecordingError> {
        if let Some(session)=&self.active {
            self.store.append_snapshot(&session.id,now,&self.cache.snapshot(session.generation))?;
            self.status.snapshot_count+=1;
        }
        Ok(())
    }
    fn fail(&mut self, error:&RecordingError) {
        if let Some(session)=self.active.take() { let _=self.store.finalize(&session.id,"storage_error",Timestamp::now()); }
        self.status.recording=false; self.status.error=Some(error.message.clone());
    }
    pub fn tick(&mut self, now: Timestamp) -> Result<(),RecordingError> {
        let Some(session)=&self.active else { return Ok(()); };
        if now.elapsed_ms<self.next_tick { return Ok(()); }
        let interval=session.interval_seconds*1000;
        if now.elapsed_ms.saturating_sub(self.next_tick)>interval*2 {
            let error=RecordingError::new("storage","Recording could not keep up with its interval and has stopped. Existing samples are saved.");
            self.fail(&error); return Err(error);
        }
        self.next_tick=now.elapsed_ms+interval;
        if let Err(e)=self.capture(now) { self.fail(&e); return Err(e); }
        Ok(())
    }
    pub fn stop(&mut self, reason:&str, now:Timestamp) -> Result<SessionMetadata,RecordingError> {
        let session=self.active.clone().ok_or_else(||RecordingError::new("stopped","No recording is running."))?;
        if let Err(e)=self.capture(now.clone()) { self.fail(&e); return Err(e); }
        if let Err(e)=self.store.finalize(&session.id,reason,now) { self.fail(&e); return Err(e); }
        self.active=None; self.status.recording=false;
        self.store.session(&session.id)
    }
    pub fn status(&self) -> RecordingStatus { self.status.clone() }
    pub fn connection(&self) -> Option<ConnectionContext> { self.connection.clone() }
    pub fn diagnostic(&mut self,generation:u64,kind:String,at:Timestamp,payload:serde_json::Value)->Result<(),RecordingError> {
        if self.connection.as_ref().map(|c|c.generation)!=Some(generation) { return Err(RecordingError::new("stale_connection","Diagnostic scan belongs to an earlier connection.")); }
        if serde_json::to_vec(&payload)?.len()>64*1024 { return Err(RecordingError::new("invalid_input","Diagnostic context is too large.")); }
        if let Some(session)=&self.active { self.store.record_diagnostic(&session.id,&kind,at.utc_ms,&payload)?; }
        self.diagnostic_events.insert(kind,(at,payload)); Ok(())
    }
}

type Work = Box<dyn FnOnce(&mut Recorder)+Send>;
/// Database and snapshot work run here, never on a scanner thread or the webview thread.
#[derive(Clone)]
pub struct RecorderHandle {
    sender:mpsc::SyncSender<Work>,
    pub cache:Arc<MeasurementCache>,
    status:Arc<Mutex<RecordingStatus>>,
}
impl RecorderHandle {
    pub fn open(path:&Path) -> Result<Self,RecordingError> {
        let path=path.to_path_buf();
        let (sender,receiver)=mpsc::sync_channel::<Work>(16);
        let (ready_send,ready_receive)=mpsc::sync_channel(1);
        let cache=Arc::new(MeasurementCache::default()); let worker_cache=cache.clone();
        let status=Arc::new(Mutex::new(RecordingStatus::default())); let worker_status=status.clone();
        std::thread::Builder::new().name("diagnostic-recordings".into()).spawn(move || {
            let store=match RecordingStore::open(&path) { Ok(s)=>s,Err(e)=>{let _=ready_send.send(Err(e));return;} };
            let mut recorder=Recorder::new(store,worker_cache);
            let _=ready_send.send(Ok(()));
            loop {
                match receiver.recv_timeout(Duration::from_millis(25)) {
                    Ok(work)=>work(&mut recorder),
                    Err(mpsc::RecvTimeoutError::Timeout)=>{},
                    Err(mpsc::RecvTimeoutError::Disconnected)=>{let _=recorder.stop("shutdown",Timestamp::now());break;}
                }
                let _=recorder.tick(Timestamp::now());
                *worker_status.lock().unwrap()=recorder.status();
            }
        }).map_err(|_|RecordingError::new("storage","Could not start the recording worker."))?;
        ready_receive.recv().map_err(|_|RecordingError::new("storage","Recording worker did not initialize."))??;
        Ok(Self{sender,cache,status})
    }
    pub fn call<T:Send+'static>(&self, work:impl FnOnce(&mut Recorder)->Result<T,RecordingError>+Send+'static)->Result<T,RecordingError> {
        let (send,receive)=mpsc::sync_channel(1);
        self.sender.try_send(Box::new(move |recorder|{let _=send.send(work(recorder));})).map_err(|_|RecordingError::new("busy","Recording worker is busy. Try again."))?;
        receive.recv().map_err(|_|RecordingError::new("storage","Recording worker stopped."))?
    }
    pub fn status(&self)->RecordingStatus { self.status.lock().unwrap().clone() }
}
