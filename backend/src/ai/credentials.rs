use super::*;
use std::{collections::BTreeMap,sync::{Arc,Mutex}};
use serde::{Serialize,Deserialize};
use zeroize::Zeroize;

pub struct SecretString(String);
impl SecretString {pub fn new(value:String)->Self{Self(value)}pub fn expose(&self)->&str{&self.0}}
impl std::fmt::Debug for SecretString {fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{f.write_str("[REDACTED]")}}
impl Clone for SecretString{fn clone(&self)->Self{Self(self.0.clone())}}
impl Drop for SecretString{fn drop(&mut self){self.0.zeroize();}}

#[derive(Debug,Clone,Copy,Serialize,Deserialize)]
#[serde(rename_all="snake_case")]
pub enum CredentialPersistence{Native,Session}
#[derive(Debug,Clone,Serialize)]
#[serde(rename_all="camelCase")]
pub struct KeyStatus{pub configured:bool,pub persistence:Option<CredentialPersistence>,pub error:Option<String>}
pub trait SecretBackend:Send+Sync {
    fn get(&self,provider:ProviderId)->Result<Option<SecretString>,AiError>;
    fn set(&self,provider:ProviderId,secret:&SecretString)->Result<(),AiError>;
    fn remove(&self,provider:ProviderId)->Result<(),AiError>;
}
pub struct NativeSecrets;
fn vault_error()->AiError{AiError::new("credential_unavailable","The operating system credential store is unavailable or locked. Unlock it, or explicitly choose a session-only key.")}
impl SecretBackend for NativeSecrets {
    fn get(&self,provider:ProviderId)->Result<Option<SecretString>,AiError>{
        let entry=keyring::v1::Entry::new("obdium.ai",provider.key()).map_err(|_|vault_error())?;
        match entry.get_password(){Ok(s)=>Ok(Some(SecretString::new(s))),Err(keyring::v1::Error::NoEntry)=>Ok(None),Err(_)=>Err(vault_error())}
    }
    fn set(&self,provider:ProviderId,secret:&SecretString)->Result<(),AiError>{keyring::v1::Entry::new("obdium.ai",provider.key()).map_err(|_|vault_error())?.set_password(secret.expose()).map_err(|_|vault_error())}
    fn remove(&self,provider:ProviderId)->Result<(),AiError>{
        let entry=keyring::v1::Entry::new("obdium.ai",provider.key()).map_err(|_|vault_error())?;
        match entry.delete_credential(){Ok(())|Err(keyring::v1::Error::NoEntry)=>Ok(()),Err(_)=>Err(vault_error())}
    }
}
pub struct CredentialVault{backend:Arc<dyn SecretBackend>,memory:Mutex<BTreeMap<ProviderId,SecretString>>,epochs:[std::sync::atomic::AtomicU64;3]}
impl Default for CredentialVault{fn default()->Self{Self::new(Arc::new(NativeSecrets))}}
impl CredentialVault{
    pub fn new(backend:Arc<dyn SecretBackend>)->Self{Self{backend,memory:Mutex::new(BTreeMap::new()),epochs:std::array::from_fn(|_|std::sync::atomic::AtomicU64::new(0))}}
    fn epoch(&self,provider:ProviderId)->&std::sync::atomic::AtomicU64{&self.epochs[match provider{ProviderId::Openai=>0,ProviderId::Anthropic=>1,ProviderId::Deepseek=>2}]}
    pub fn revision(&self,provider:ProviderId)->u64{self.epoch(provider).load(std::sync::atomic::Ordering::SeqCst)}
    pub fn set(&self,provider:ProviderId,secret:SecretString,persistence:CredentialPersistence)->Result<(),AiError>{
        if secret.expose().trim().is_empty() || secret.expose().len()>4096 || secret.expose().chars().any(char::is_control){return Err(AiError::new("invalid_input","Enter a valid API key."));}
        self.epoch(provider).fetch_add(1,std::sync::atomic::Ordering::SeqCst);
        match persistence{CredentialPersistence::Native=>{self.backend.set(provider,&secret)?;self.memory.lock().unwrap().remove(&provider);},CredentialPersistence::Session=>{self.memory.lock().unwrap().insert(provider,secret);}}Ok(())
    }
    pub fn get(&self,provider:ProviderId)->Result<SecretString,AiError>{
        if let Some(secret)=self.memory.lock().unwrap().get(&provider){return Ok(secret.clone());}
        self.backend.get(provider)?.ok_or_else(||AiError::new("auth","Configure an API key for this provider in Settings."))
    }
    pub fn status(&self,provider:ProviderId)->KeyStatus{
        if self.memory.lock().unwrap().contains_key(&provider){return KeyStatus{configured:true,persistence:Some(CredentialPersistence::Session),error:None};}
        match self.backend.get(provider){Ok(Some(_))=>KeyStatus{configured:true,persistence:Some(CredentialPersistence::Native),error:None},Ok(None)=>KeyStatus{configured:false,persistence:None,error:None},Err(e)=>KeyStatus{configured:false,persistence:None,error:Some(e.message)}}
    }
    pub fn remove(&self,provider:ProviderId)->Result<(),AiError>{self.epoch(provider).fetch_add(1,std::sync::atomic::Ordering::SeqCst);self.memory.lock().unwrap().remove(&provider);self.backend.remove(provider)}
}
