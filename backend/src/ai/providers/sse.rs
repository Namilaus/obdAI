use crate::ai::AiError;
#[derive(Debug,Clone)]
pub struct SseEvent{pub event:Option<String>,pub data:String}
#[derive(Default)]
pub struct SseDecoder{buffer:Vec<u8>}
impl SseDecoder{
    pub fn push(&mut self,chunk:&[u8])->Result<Vec<SseEvent>,AiError>{
        self.buffer.extend_from_slice(chunk);let mut out=Vec::new();
        loop{
            let boundary=[self.buffer.windows(2).position(|w|w==b"\n\n").map(|i|(i,2)),self.buffer.windows(4).position(|w|w==b"\r\n\r\n").map(|i|(i,4))].into_iter().flatten().min_by_key(|v|v.0);
            let Some((end,delimiter))=boundary else{break;};
            if end>1024*1024{return Err(AiError::new("protocol","Provider streaming event exceeded its limit."));}
            let bytes:Vec<_>=self.buffer.drain(..end+delimiter).collect();
            let text=std::str::from_utf8(&bytes).map_err(|_|AiError::new("protocol","Provider returned invalid UTF-8."))?;
            let mut data=Vec::new();let mut event=None;
            for line in text.lines(){if let Some(value)=line.strip_prefix("data:"){data.push(value.strip_prefix(' ').unwrap_or(value));}if let Some(value)=line.strip_prefix("event:"){event=Some(value.trim().to_string());}}
            if !data.is_empty(){out.push(SseEvent{event,data:data.join("\n")});}
        }
        if self.buffer.len()>1024*1024{return Err(AiError::new("protocol","Provider streaming event exceeded its limit."));}Ok(out)
    }
    pub fn finish(&self)->Result<(),AiError>{if self.buffer.iter().all(u8::is_ascii_whitespace){Ok(())}else{Err(AiError::new("protocol","Provider stream ended in an incomplete event."))}}
}
