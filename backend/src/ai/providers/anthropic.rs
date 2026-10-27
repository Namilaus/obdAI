use super::{*,common::*,transport::*};
use serde_json::{Value,json};
use std::{collections::BTreeMap,sync::{Arc,atomic::{AtomicBool,Ordering}}};

pub struct AnthropicProvider{transport:Arc<dyn HttpTransport>}
impl AnthropicProvider{pub fn new(transport:Arc<dyn HttpTransport>)->Self{Self{transport}}}
fn input(messages:&[ChatMessage])->Result<Vec<Value>,AiError>{
    let mut out=Vec::new();
    for message in messages{
        if let Some(wire)=message.extra.as_ref().filter(|e|e["kind"]=="provider_reply").map(|e|e["wire"].clone()){out.push(wire);}
        else if message.role=="tool"{
            let id=message.extra.as_ref().and_then(|e|e["callId"].as_str()).ok_or_else(||protocol("Stored Claude tool-result identity is missing."))?;
            let block=json!({"type":"tool_result","tool_use_id":id,"content":message.content});
            if let Some(last)=out.last_mut().filter(|m|m["role"]=="user"){last["content"].as_array_mut().ok_or_else(||protocol("Stored Claude user content is invalid."))?.push(block);}
            else{out.push(json!({"role":"user","content":[block]}));}
        }else{out.push(json!({"role":message.role,"content":[{"type":"text","text":message.content}]}));}
    }Ok(out)
}
fn assemble(events:Vec<Value>)->Result<Value,AiError>{
    let mut blocks=Vec::new();let mut arguments:BTreeMap<usize,String>=BTreeMap::new();let mut usage=json!({});let mut stop=Value::Null;let mut completed=false;
    for event in events{
        match event["type"].as_str().unwrap_or_default(){
            "error"=>return Err(AiError::new("provider","Claude stopped this response with an error.")),
            "message_start"=>usage=event["message"]["usage"].clone(),
            "content_block_start"=>{let index=event["index"].as_u64().ok_or_else(||protocol("Claude block index missing."))? as usize;if index!=blocks.len()||index>=64{return Err(protocol("Claude block ordering was invalid."));}blocks.push(event["content_block"].clone());},
            "content_block_delta"=>{
                let index=event["index"].as_u64().ok_or_else(||protocol("Claude block index missing."))? as usize;
                let block=blocks.get_mut(index).ok_or_else(||protocol("Claude block delta arrived before its block."))?;let delta=&event["delta"];
                let pair=match delta["type"].as_str(){Some("text_delta")=>Some(("text","text")),Some("thinking_delta")=>Some(("thinking","thinking")),Some("signature_delta")=>Some(("signature","signature")),_=>None};
                if let Some((field,source))=pair{let mut text=block[field].as_str().unwrap_or_default().to_string();text.push_str(delta[source].as_str().unwrap_or_default());block[field]=json!(text);}
                if delta["type"]=="input_json_delta"{arguments.entry(index).or_default().push_str(delta["partial_json"].as_str().unwrap_or_default());}
            },
            "message_delta"=>{stop=event["delta"]["stop_reason"].clone();if let Some(output)=event["usage"]["output_tokens"].as_u64(){usage["output_tokens"]=json!(output);}},
            "message_stop"=>completed=true,
            _=>{},
        }
    }
    if !completed{return Err(protocol("Claude stream ended before a completed message."));}
    for(index,input)in arguments{blocks[index]["input"]=serde_json::from_str(&input).map_err(|_|protocol("Claude tool arguments were incomplete."))?;}
    Ok(json!({"content":blocks,"stop_reason":stop,"usage":usage}))
}
fn parse(value:Value)->Result<ProviderReply,AiError>{
    let blocks=value["content"].as_array().ok_or_else(||protocol("Claude returned no complete content."))?;
    let mut text=String::new();let mut calls=Vec::new();
    for block in blocks{if block["type"]=="text"{text.push_str(block["text"].as_str().unwrap_or_default());}if block["type"]=="tool_use"{calls.push(parse_call(block["id"].as_str().unwrap_or_default(),block["name"].as_str().unwrap_or_default(),&block["input"].to_string())?);}}
    if value["stop_reason"]=="refusal"{return Err(AiError::new("refusal","The selected Claude model declined this request. Try clarifying the symptoms."));}
    let truncated=value["stop_reason"]=="max_tokens";if truncated&&!calls.is_empty(){return Err(protocol("Claude stopped before its tool calls completed."));}check_calls(&calls)?;
    Ok(ProviderReply{text,calls,wire:json!({"role":"assistant","content":blocks}),usage:TokenUsage{input_tokens:value["usage"]["input_tokens"].as_u64().unwrap_or(0),output_tokens:value["usage"]["output_tokens"].as_u64().unwrap_or(0)},truncated})
}
#[async_trait::async_trait]
impl ProviderAdapter for AnthropicProvider{
    async fn list_models(&self,secret:&SecretString,cancel:CancellationToken)->Result<Vec<ModelInfo>,AiError>{models(self.transport.as_ref(),ProviderId::Anthropic,secret,cancel).await}
    async fn respond(&self,request:ProviderRequest,secret:&SecretString,cancel:CancellationToken,sink:TextSink)->Result<ProviderReply,AiError>{
        let tools:Vec<_>=request.tools.iter().map(|t|json!({"name":t.name,"description":t.description,"input_schema":t.parameters})).collect();
        let mut body=json!({"model":request.model,"system":request.system,"messages":input(&request.messages)?,"max_tokens":request.max_output_tokens,"tools":tools,"stream":true});
        if let Some(name)=request.forced_tool{body["tool_choice"]=json!({"type":"tool","name":name});}
        let emitted=Arc::new(AtomicBool::new(false));let seen=emitted.clone();let live=sink.clone();
        let reply=self.transport.execute(HttpRequest{provider:ProviderId::Anthropic,kind:RequestKind::Generate,body:Some(body)},secret,cancel,Arc::new(move|e|{if e["type"]=="content_block_delta"&&e["delta"]["type"]=="text_delta"{if let Some(text)=e["delta"]["text"].as_str(){seen.store(true,Ordering::Relaxed);live(text.into());}}})).await?;
        let value=match reply{HttpReply::Json(v)=>v,HttpReply::Stream(events)=>assemble(events)?};let result=parse(value)?;
        if !emitted.load(Ordering::Relaxed)&&!result.text.is_empty(){sink(result.text.clone());}Ok(result)
    }
}
