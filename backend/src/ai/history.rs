use super::*;
use crate::recording::RecordingStore;
use sqlite::State;

pub struct HistoryStore<'a>{store:&'a RecordingStore}
impl<'a> HistoryStore<'a>{
    pub fn new(store:&'a RecordingStore)->Self{Self{store}}
    pub fn initialize(store:&RecordingStore)->Result<(),AiError>{
        store.connection.execute(include_str!("migrations/002_conversations.sql"))?;
        store.connection.execute("UPDATE messages SET status='interrupted' WHERE status='pending'; UPDATE conversations SET active_turn=NULL,status='interrupted',revision=revision+1 WHERE active_turn IS NOT NULL;")?;Ok(())
    }
    pub fn create(&self,selection:ModelSelection,mut ids:Vec<String>,profile:String)->Result<Conversation,AiError>{
        ids.sort();ids.dedup();
        if ids.len()>10 || profile.chars().count()>8000 || selection.model_id.is_empty() || selection.model_id.len()>256 || selection.model_id.chars().any(char::is_control){return Err(AiError::new("invalid_input","Choose a model, up to 10 recordings, and a description up to 8,000 characters."));}
        let mut vehicle=None;
        for id in &ids {let session=self.store.session(id)?;if vehicle.as_ref().map(|v|v!=&session.vehicle_id).unwrap_or(false){return Err(AiError::new("vehicle_mismatch","Assign these recordings to the same vehicle before combining them."));}vehicle=Some(session.vehicle_id);}
        let conversation=Conversation{id:uuid::Uuid::new_v4().to_string(),selection,recording_ids:ids.clone(),title:profile.chars().take(80).collect(),profile,created_utc_ms:chrono::Utc::now().timestamp_millis(),status:"ready".into()};
        self.store.connection.execute("BEGIN IMMEDIATE")?;
        let result=(||->Result<(),AiError>{
            let mut s=self.store.connection.prepare("INSERT INTO conversations(id,metadata,vehicle_id) VALUES(?,?,?)")?;
            s.bind((1,conversation.id.as_str()))?;s.bind((2,serde_json::to_string(&conversation)?.as_str()))?;s.bind((3,vehicle.as_deref()))?;s.next()?;
            for id in ids {let mut s=self.store.connection.prepare("INSERT INTO conversation_recordings(conversation_id,session_id) VALUES(?,?)")?;s.bind((1,conversation.id.as_str()))?;s.bind((2,id.as_str()))?;s.next()?;}Ok(())
        })();
        match result{Ok(())=>{self.store.connection.execute("COMMIT")?;Ok(conversation)},Err(e)=>{let _=self.store.connection.execute("ROLLBACK");Err(e)}}
    }
    pub fn get(&self,id:&str)->Result<Conversation,AiError>{
        let mut s=self.store.connection.prepare("SELECT metadata,status FROM conversations WHERE id=?")?;s.bind((1,id))?;
        if s.next()?!=State::Row{return Err(AiError::new("deleted","This conversation no longer exists."));}
        let mut chat:Conversation=serde_json::from_str(&s.read::<String,_>(0)?)?;chat.status=s.read(1)?;Ok(chat)
    }
    pub fn list(&self)->Result<Vec<Conversation>,AiError>{
        let mut s=self.store.connection.prepare("SELECT metadata,status FROM conversations ORDER BY rowid DESC LIMIT 100")?;let mut out=Vec::new();
        while s.next()?==State::Row{let mut chat:Conversation=serde_json::from_str(&s.read::<String,_>(0)?)?;chat.status=s.read(1)?;out.push(chat);}Ok(out)
    }
    pub fn scope(&self,id:&str)->Result<ConversationScope,AiError>{
        let mut s=self.store.connection.prepare("SELECT vehicle_id,revision FROM conversations WHERE id=?")?;s.bind((1,id))?;
        if s.next()?!=State::Row{return Err(AiError::new("deleted","This conversation no longer exists."));}
        let vehicle_id=s.read::<Option<String>,_>(0)?;let revision=s.read(1)?;
        let mut s=self.store.connection.prepare("SELECT session_id FROM conversation_recordings WHERE conversation_id=? ORDER BY session_id")?;s.bind((1,id))?;let mut ids=Vec::new();
        while s.next()?==State::Row{ids.push(s.read(0)?);}
        Ok(ConversationScope{conversation_id:id.into(),recording_ids:ids,vehicle_id,revision})
    }
    pub fn begin_turn(&self,id:&str,text:String)->Result<TurnLease,AiError>{
        if text.trim().is_empty() || text.chars().count()>16000{return Err(AiError::new("invalid_input","Enter a message up to 16,000 characters."));}
        self.get(id)?;let turn_id=uuid::Uuid::new_v4().to_string();
        self.store.connection.execute("BEGIN IMMEDIATE")?;
        let result=(||->Result<TurnLease,AiError>{
            let mut s=self.store.connection.prepare("UPDATE conversations SET active_turn=?,status='running',revision=revision+1 WHERE id=? AND active_turn IS NULL")?;
            s.bind((1,turn_id.as_str()))?;s.bind((2,id))?;s.next()?;
            if self.store.connection.change_count()!=1{return Err(AiError::new("busy","A response is already running in this conversation."));}
            let lease=TurnLease{conversation_id:id.into(),turn_id,revision:self.scope(id)?.revision};
            self.append(&lease,"user",&text,None)?;Ok(lease)
        })();
        match result{Ok(lease)=>{self.store.connection.execute("COMMIT")?;Ok(lease)},Err(e)=>{let _=self.store.connection.execute("ROLLBACK");Err(e)}}
    }
    pub fn ensure_lease(&self,lease:&TurnLease)->Result<(),AiError>{
        let mut s=self.store.connection.prepare("SELECT active_turn FROM conversations WHERE id=?")?;s.bind((1,lease.conversation_id.as_str()))?;
        if s.next()?!=State::Row || s.read::<Option<String>,_>(0)?.as_deref()!=Some(lease.turn_id.as_str()){return Err(AiError::new("cancelled","This request was cancelled or its history was deleted."));}Ok(())
    }
    pub fn append(&self,lease:&TurnLease,role:&str,content:&str,extra:Option<serde_json::Value>)->Result<(),AiError>{
        self.ensure_lease(lease)?;
        if content.len()>256*1024 || extra.as_ref().map(|e|e.to_string().len()>512*1024).unwrap_or(false){return Err(AiError::new("context_limit","Response exceeded the local history limit."));}
        let mut s=self.store.connection.prepare("INSERT INTO messages(conversation_id,turn_id,role,content,extra) VALUES(?,?,?,?,?)")?;
        s.bind((1,lease.conversation_id.as_str()))?;s.bind((2,lease.turn_id.as_str()))?;s.bind((3,role))?;s.bind((4,content))?;s.bind((5,extra.map(|e|e.to_string()).as_deref()))?;s.next()?;Ok(())
    }
    pub fn finish(&self,lease:&TurnLease,status:&str)->Result<(),AiError>{
        self.ensure_lease(lease)?;
        let mut s=self.store.connection.prepare("UPDATE messages SET status=? WHERE conversation_id=? AND turn_id=?")?;s.bind((1,status))?;s.bind((2,lease.conversation_id.as_str()))?;s.bind((3,lease.turn_id.as_str()))?;s.next()?;
        let mut s=self.store.connection.prepare("UPDATE conversations SET active_turn=NULL,status=?,revision=revision+1 WHERE id=? AND active_turn=?")?;s.bind((1,status))?;s.bind((2,lease.conversation_id.as_str()))?;s.bind((3,lease.turn_id.as_str()))?;s.next()?;Ok(())
    }
    pub fn messages(&self,id:&str)->Result<Vec<ChatMessage>,AiError>{
        self.get(id)?;let mut s=self.store.connection.prepare("SELECT turn_id,role,content,extra,status FROM messages WHERE conversation_id=? ORDER BY id")?;s.bind((1,id))?;let mut out=Vec::new();
        while s.next()?==State::Row{out.push(ChatMessage{turn_id:s.read(0)?,role:s.read(1)?,content:s.read(2)?,extra:s.read::<Option<String>,_>(3)?.map(|v|serde_json::from_str(&v)).transpose()?,status:s.read(4)?});}Ok(out)
    }
    pub fn delete(&self,id:&str)->Result<(),AiError>{self.get(id)?;let mut s=self.store.connection.prepare("DELETE FROM conversations WHERE id=?")?;s.bind((1,id))?;s.next()?;Ok(())}
    pub fn revoke_all(&self)->Result<(),AiError>{self.store.connection.execute("UPDATE messages SET status='cancelled' WHERE status='pending';UPDATE conversations SET active_turn=NULL,status='cancelled',revision=revision+1 WHERE active_turn IS NOT NULL;")?;Ok(())}
}
