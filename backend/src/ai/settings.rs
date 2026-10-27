use super::*;
use crate::recording::RecordingStore;
use sqlite::State;
pub struct SettingsStore<'a>{store:&'a RecordingStore}
impl<'a> SettingsStore<'a>{
    pub fn new(store:&'a RecordingStore)->Self{Self{store}}
    pub fn load(&self)->Result<AiPreferences,AiError>{
        let mut s=self.store.connection.prepare("SELECT payload FROM ai_preferences WHERE id=1")?;
        if s.next()?==State::Row{Ok(serde_json::from_str(&s.read::<String,_>(0)?)?)}else{Ok(AiPreferences::default())}
    }
    pub fn save(&self,preferences:&AiPreferences)->Result<(),AiError>{
        let mut s=self.store.connection.prepare("INSERT INTO ai_preferences(id,payload) VALUES(1,?) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload")?;
        s.bind((1,serde_json::to_string(preferences)?.as_str()))?;s.next()?;Ok(())
    }
}
