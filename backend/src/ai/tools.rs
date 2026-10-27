use super::*;
use crate::recording::*;
use serde::{Deserialize,Serialize};
use serde_json::{Value,json};
use tokio_util::sync::CancellationToken;

#[derive(Deserialize)]#[serde(deny_unknown_fields)]struct Empty{}
#[derive(Deserialize)]#[serde(rename_all="camelCase",deny_unknown_fields)]
struct MetricArgs{session_id:String,#[serde(default)]offset:usize,#[serde(default="twenty")]limit:usize}
#[derive(Deserialize)]#[serde(rename_all="camelCase",deny_unknown_fields)]
struct QueryArgs{session_id:String,#[serde(default)]metric_ids:Vec<String>,#[serde(default)]start_elapsed_ms:u64,end_elapsed_ms:Option<u64>,#[serde(default="fifty")]limit:usize,#[serde(default)]cursor:i64,#[serde(default)]offset:usize}
#[derive(Deserialize)]#[serde(rename_all="camelCase",deny_unknown_fields)]struct ContextArgs{session_id:Option<String>}
fn twenty()->usize{20}fn fifty()->usize{50}
fn args<T:serde::de::DeserializeOwned>(value:Value)->Result<T,AiError>{serde_json::from_value(value).map_err(|_|AiError::new("invalid_input","Tool arguments are invalid or contain unsupported fields."))}
fn authorized(scope:&ConversationScope,id:&str)->Result<(),AiError>{if scope.recording_ids.iter().any(|s|s==id){Ok(())}else{Err(AiError::new("forbidden","This recording is not selected for this conversation."))}}
fn page<T:Serialize>(items:&[T],offset:usize,limit:usize,mut envelope:Value,max:usize)->Result<Value,AiError>{
    if offset>items.len()||limit==0||limit>100{return Err(AiError::new("invalid_input","Invalid result page."));}
    let mut end=(offset+limit).min(items.len());
    loop{envelope["items"]=serde_json::to_value(&items[offset..end])?;envelope["truncated"]=json!(end<items.len());envelope["nextOffset"]=if end<items.len(){json!(end)}else{Value::Null};
        if serde_json::to_vec(&envelope)?.len()<=max{return Ok(envelope);}if end==offset{return Err(AiError::new("context_limit","Result cannot fit the current context. Request fewer metrics or a shorter time range."));}end-=1;}
}
pub struct ToolRouter;
impl ToolRouter{
    pub fn definitions()->Vec<ToolDefinition>{
        let session=json!({"type":"string","description":"One of the selected session IDs; never guess an ID."});
        let query=json!({"type":"object","properties":{"sessionId":session,"metricIds":{"type":"array","items":{"type":"string"},"maxItems":10},"startElapsedMs":{"type":"integer","minimum":0},"endElapsedMs":{"type":"integer","minimum":0},"limit":{"type":"integer","minimum":1,"maximum":500},"cursor":{"type":"integer","minimum":0},"offset":{"type":"integer","minimum":0}},"required":["sessionId"],"additionalProperties":false});
        vec![
            ToolDefinition{name:"list_selected_recordings".into(),description:"List this chat's selected recordings and source modes. No other history is accessible.".into(),parameters:json!({"type":"object","properties":{},"additionalProperties":false})},
            ToolDefinition{name:"list_recording_metrics".into(),description:"Discover exact metric IDs, units and availability. Use nextOffset to retrieve more.".into(),parameters:json!({"type":"object","properties":{"sessionId":session,"offset":{"type":"integer","minimum":0},"limit":{"type":"integer","minimum":1,"maximum":50}},"required":["sessionId"],"additionalProperties":false})},
            ToolDefinition{name:"get_recording_summary".into(),description:"Summarize unique valid observations over a half-open time range in milliseconds since recording start. Includes missing/stale coverage; offset pages metrics.".into(),parameters:query.clone()},
            ToolDefinition{name:"get_recording_samples".into(),description:"Read detailed samples with acquisition time, age and provenance. Use nextCursor to continue; repeated snapshots are not new observations.".into(),parameters:query},
            ToolDefinition{name:"get_diagnostic_context".into(),description:"Read recorded vehicle details, fault-code and readiness scans with timestamps. Missing scans are unknown, never a clear vehicle.".into(),parameters:json!({"type":"object","properties":{"sessionId":session},"additionalProperties":false})},
        ]
    }
    pub fn execute(store:&RecordingStore,scope:&ConversationScope,call:ToolCall,max_bytes:usize)->Result<Value,AiError>{Self::execute_cancellable(store,scope,call,max_bytes,CancellationToken::new())}
    pub fn execute_cancellable(store:&RecordingStore,scope:&ConversationScope,call:ToolCall,max_bytes:usize,cancel:CancellationToken)->Result<Value,AiError>{
        let current=HistoryStore::new(store).scope(&scope.conversation_id)?;
        if current.recording_ids!=scope.recording_ids||current.vehicle_id!=scope.vehicle_id{return Err(AiError::new("cancelled","Conversation access changed."));}
        if cancel.is_cancelled(){return Err(AiError::new("cancelled","Query cancelled."));}
        let max_bytes=max_bytes.min(64*1024);
        let result=match call.name.as_str(){
            "list_selected_recordings"=>{
                let _:Empty=args(call.arguments)?;
                let mut sessions=Vec::new();for id in &scope.recording_ids{let s=store.session(id)?;sessions.push(json!({"sessionId":s.id,"label":s.label,"source":s.source,"simulated":s.simulated,"intervalSeconds":s.interval_seconds,"startedUtcMs":s.started_at.utc_ms,"endedUtcMs":s.ended_at.map(|t|t.utc_ms),"snapshotCount":s.snapshot_count}));}
                page(&sessions,0,10,json!({"kind":"selected_recordings"}),max_bytes)?
            },
            "list_recording_metrics"=>{
                let a:MetricArgs=args(call.arguments)?;authorized(scope,&a.session_id)?;if a.limit>50{return Err(AiError::new("invalid_input","Request up to 50 metrics per page."));}
                let metrics:Vec<_>=store.metrics(&a.session_id)?.into_iter().map(|m|json!({"metricId":m.metric_id,"name":m.name,"unit":m.unit,"availability":m.availability,"latestAcquiredAt":m.acquired_at})).collect();
                page(&metrics,a.offset,a.limit,json!({"sessionId":a.session_id}),max_bytes)?
            },
            "get_recording_summary"=>{
                let a:QueryArgs=args(call.arguments)?;authorized(scope,&a.session_id)?;
                let summaries=store.summary_cancellable(&a.session_id,&a.metric_ids,a.start_elapsed_ms,a.end_elapsed_ms.unwrap_or(u64::MAX),||cancel.is_cancelled())?;
                page(&summaries,a.offset,a.limit.min(10),json!({"sessionId":a.session_id,"startElapsedMs":a.start_elapsed_ms,"endElapsedMs":a.end_elapsed_ms,"source":store.session(&a.session_id)?.source}),max_bytes)?
            },
            "get_recording_samples"=>{
                let a:QueryArgs=args(call.arguments)?;authorized(scope,&a.session_id)?;
                let mut samples=store.samples(&a.session_id,&a.metric_ids,a.start_elapsed_ms,a.end_elapsed_ms.unwrap_or(u64::MAX),a.limit,a.cursor)?;
                loop{let value=json!({"sessionId":a.session_id,"source":store.session(&a.session_id)?.source,"samples":samples});if serde_json::to_vec(&value)?.len()<=max_bytes{break value;}
                    if samples.rows.len()<=1{return Err(AiError::new("context_limit","A sample does not fit the remaining context. Use a summary first."));}
                    samples.rows.pop();samples.truncated=true;samples.next_cursor=samples.rows.last().map(|r|r.row_id);
                }
            },
            "get_diagnostic_context"=>{
                let a:ContextArgs=args(call.arguments)?;let ids=if let Some(id)=a.session_id{authorized(scope,&id)?;vec![id]}else{scope.recording_ids.clone()};let mut contexts=Vec::new();
                for id in ids{contexts.push(json!({"sessionId":id,"context":store.diagnostic_context(&id)?}));}
                json!({"recordings":contexts,"hasScannerEvidence":!scope.recording_ids.is_empty()})
            },
            _=>return Err(AiError::new("forbidden","This tool is not available. Only read-only recording tools are allowed.")),
        };
        if serde_json::to_vec(&result)?.len()>max_bytes{return Err(AiError::new("context_limit","Result exceeded the remaining context; request one recording or a smaller selection."));}Ok(result)
    }
}
