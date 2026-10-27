use obdium::{ai::*,recording::RecordingStore};
use std::sync::Arc;
struct Locked;
impl SecretBackend for Locked {
 fn get(&self,_:ProviderId)->Result<Option<SecretString>,AiError>{Err(AiError::new("credential_unavailable","Locked"))}
 fn set(&self,_:ProviderId,_:&SecretString)->Result<(),AiError>{Err(AiError::new("credential_unavailable","Locked"))}
 fn remove(&self,_:ProviderId)->Result<(),AiError>{Err(AiError::new("credential_unavailable","Locked"))}
}
#[test]
fn locked_native_store_does_not_silently_save_plaintext() {
 let vault=CredentialVault::new(Arc::new(Locked));
 assert!(vault.set(ProviderId::Openai,SecretString::new("test-secret".into()),CredentialPersistence::Native).is_err());
 assert!(!vault.status(ProviderId::Openai).configured);
 vault.set(ProviderId::Openai,SecretString::new("test-secret".into()),CredentialPersistence::Session).unwrap();
 assert!(vault.status(ProviderId::Openai).configured);
 assert!(!format!("{:?}",vault.get(ProviderId::Openai).unwrap()).contains("test-secret"));
}
#[test]
fn preferences_default_disabled_and_contain_no_credentials() {
 let store=RecordingStore::open(std::path::Path::new(":memory:")).unwrap();HistoryStore::initialize(&store).unwrap();
 let preferences=SettingsStore::new(&store).load().unwrap();assert!(!preferences.enabled);
 assert!(!serde_json::to_string(&preferences).unwrap().contains("test-secret"));
}
#[test]
fn sse_decoder_handles_split_unicode_and_rejects_overlong_events() {
 let mut parser=SseDecoder::default();let mut events=Vec::new();
 for byte in "event: text\ndata: {\"text\":\"90 °C\"}\n\n".as_bytes(){events.extend(parser.push(&[*byte]).unwrap());}
 assert_eq!(events[0].data,"{\"text\":\"90 °C\"}");
 assert!(parser.push(&vec![b'x';1024*1024+1]).is_err());
}
#[test]
fn sse_preserves_separate_events_with_mixed_newline_styles(){
 let mut parser=SseDecoder::default();
 let events=parser.push(b"data: first\r\n\r\ndata: second\n\n").unwrap();
 assert_eq!(events.len(),2);assert_eq!(events[0].data,"first");assert_eq!(events[1].data,"second");
}
