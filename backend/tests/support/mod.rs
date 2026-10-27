use obdium::ai::{*,providers::transport::*};
use std::{collections::VecDeque,sync::Mutex};
use tokio_util::sync::CancellationToken;
pub struct MockTransport{pub requests:Mutex<Vec<HttpRequest>>,pub replies:Mutex<VecDeque<HttpReply>>}
impl MockTransport{pub fn new(replies:Vec<HttpReply>)->Self{Self{requests:Mutex::new(Vec::new()),replies:Mutex::new(replies.into())}}}
#[async_trait::async_trait]
impl HttpTransport for MockTransport{
 async fn execute(&self,request:HttpRequest,_:&SecretString,cancel:CancellationToken,sink:WireSink)->Result<HttpReply,AiError>{
  if cancel.is_cancelled(){return Err(AiError::new("cancelled","Cancelled"));}
  self.requests.lock().unwrap().push(request);
  let reply=self.replies.lock().unwrap().pop_front().ok_or_else(||AiError::new("fixture","No provider fixture remains"))?;
  if let HttpReply::Stream(events)=&reply{for event in events{sink(event);}}
  Ok(reply)
 }
}
pub fn request()->ProviderRequest{ProviderRequest{model:"fixture-model".into(),system:"Diagnostic test".into(),messages:vec![ChatMessage{role:"user".into(),content:"Check selected readings".into(),extra:None,turn_id:"t".into(),status:"pending".into()}],tools:vec![],forced_tool:None,max_output_tokens:512}}
