use obdium::{ai::*,ai::providers::validate_model};
use serde::Deserialize;
use serde_json::{Value,json};
use std::sync::Arc;
use tauri::Emitter;

fn service()->Result<Arc<AssistantService>,AiError>{AI_SERVICE.get().cloned().ok_or_else(||AiError::new("unavailable","AI services are unavailable. Check local storage and restart the application."))}
#[derive(Deserialize)]#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct PreferencesUpdate{enabled:bool,default_selection:Option<ModelSelection>,selections:std::collections::BTreeMap<ProviderId,String>}
#[tauri::command]
pub async fn ai_preferences_get()->Result<AiPreferences,AiError>{service()?.read(|store|SettingsStore::new(store).load()).await}
#[tauri::command]
pub async fn ai_preferences_set(update:PreferencesUpdate)->Result<AiPreferences,AiError>{
    if update.selections.values().any(|id|id.len()>256||id.chars().any(char::is_control))||update.default_selection.as_ref().map(|s|s.model_id.is_empty()||s.model_id.len()>256||s.model_id.chars().any(char::is_control)).unwrap_or(false){return Err(AiError::new("invalid_input","Choose a valid model ID."));}
    let service=service()?;if !update.enabled{service.cancel_all();}
    service.database(move|r|{let settings=SettingsStore::new(&r.store);let mut prefs=settings.load()?;if !update.enabled{HistoryStore::new(&r.store).revoke_all()?;}prefs.enabled=update.enabled;prefs.default_selection=update.default_selection;prefs.selections=update.selections;settings.save(&prefs)?;Ok(prefs)}).await
}
#[tauri::command]
pub async fn ai_key_status(provider:ProviderId)->Result<KeyStatus,AiError>{let vault=service()?.vault.clone();tokio::task::spawn_blocking(move||vault.status(provider)).await.map_err(|_|AiError::new("credential_unavailable","Credential status request was interrupted."))}
async fn invalidate_provider(service:&AssistantService,provider:ProviderId)->Result<(),AiError>{
    service.cancel_provider(provider);service.database(move|r|{HistoryStore::new(&r.store).revoke_provider(provider)?;let settings=SettingsStore::new(&r.store);let mut prefs=settings.load()?;prefs.validated_models.remove(&provider);settings.save(&prefs)}).await
}
#[tauri::command]
pub async fn ai_key_set(provider:ProviderId,secret:String,persistence:CredentialPersistence)->Result<KeyStatus,AiError>{
    let service=service()?;invalidate_provider(&service,provider).await?;let vault=service.vault.clone();
    tokio::task::spawn_blocking(move||{vault.set(provider,SecretString::new(secret),persistence)?;Ok(vault.status(provider))}).await.map_err(|_|AiError::new("credential_unavailable","Credential request was interrupted."))?
}
#[tauri::command]
pub async fn ai_key_remove(provider:ProviderId)->Result<KeyStatus,AiError>{
    let service=service()?;invalidate_provider(&service,provider).await?;let vault=service.vault.clone();tokio::task::spawn_blocking(move||{vault.remove(provider)?;Ok(vault.status(provider))}).await.map_err(|_|AiError::new("credential_unavailable","Credential request was interrupted."))?
}
#[tauri::command]
pub async fn ai_disclosure_accept(provider:ProviderId)->Result<(),AiError>{service()?.database(move|r|{let settings=SettingsStore::new(&r.store);let mut prefs=settings.load()?;prefs.disclosed_providers.insert(provider);settings.save(&prefs)}).await}

async fn provider_credentials(service:&AssistantService,provider:ProviderId)->Result<(SecretString,u64),AiError>{
    if !service.read(|store|Ok(SettingsStore::new(store).load()?.enabled)).await?{return Err(AiError::new("ai_disabled","Enable AI before contacting a provider."));}
    let revision=service.vault.revision(provider);let vault=service.vault.clone();let secret=tokio::task::spawn_blocking(move||vault.get(provider)).await.map_err(|_|AiError::new("auth","Credential request was interrupted."))??;Ok((secret,revision))
}
#[tauri::command]
pub async fn ai_models_refresh(provider:ProviderId)->Result<Vec<ModelInfo>,AiError>{
    let service=service()?;let(secret,revision)=provider_credentials(&service,provider).await?;let adapter=service.adapter(provider)?;let(id,cancel)=service.register_auxiliary(provider);
    let result=tokio::time::timeout(std::time::Duration::from_secs(180),adapter.list_models(&secret,cancel.clone())).await.map_err(|_|AiError::new("timeout","Model discovery timed out."));service.finish_auxiliary(&id);
    let models=result??;if cancel.is_cancelled()||service.vault.revision(provider)!=revision{return Err(AiError::new("cancelled","Provider settings changed during model discovery."));}
    let saved=models.clone();service.database(move|r|{let settings=SettingsStore::new(&r.store);let mut prefs=settings.load()?;if !prefs.enabled{return Err(AiError::new("ai_disabled","AI was disabled."));}prefs.models.insert(provider,saved);settings.save(&prefs)}).await?;Ok(models)
}
#[tauri::command]
pub async fn ai_model_test(provider:ProviderId,model_id:String)->Result<Value,AiError>{
    if model_id.trim().is_empty()||model_id.len()>256||model_id.chars().any(char::is_control){return Err(AiError::new("invalid_input","Choose or enter a model ID."));}
    let service=service()?;let(secret,revision)=provider_credentials(&service,provider).await?;let adapter=service.adapter(provider)?;let(id,cancel)=service.register_auxiliary(provider);
    let result=tokio::time::timeout(std::time::Duration::from_secs(180),validate_model(adapter.as_ref(),model_id.clone(),&secret,cancel.clone())).await.map_err(|_|AiError::new("timeout","Compatibility test timed out. A provider charge may already have occurred."));service.finish_auxiliary(&id);
    let usage=result??;if cancel.is_cancelled()||service.vault.revision(provider)!=revision{return Err(AiError::new("cancelled","Provider credentials changed during the test."));}
    service.database(move|r|{let settings=SettingsStore::new(&r.store);let mut prefs=settings.load()?;if !prefs.enabled{return Err(AiError::new("ai_disabled","AI was disabled."));}let ids=prefs.validated_models.entry(provider).or_default();if !ids.contains(&model_id){ids.push(model_id);}settings.save(&prefs)}).await?;Ok(json!({"compatible":true,"usage":usage}))
}
#[tauri::command]
pub async fn ai_conversation_create(selection:ModelSelection,recording_ids:Vec<String>,profile:String)->Result<Conversation,AiError>{service()?.database(move|r|HistoryStore::new(&r.store).create(selection,recording_ids,profile)).await}
#[tauri::command]
pub async fn ai_conversation_list()->Result<Vec<Conversation>,AiError>{service()?.read(|store|HistoryStore::new(store).list()).await}
#[tauri::command]
pub async fn ai_conversation_get(id:String)->Result<Value,AiError>{service()?.read(move|store|{
    let history=HistoryStore::new(store);let conversation=history.get(&id)?;
    let messages:Vec<_>=history.messages(&id)?.into_iter().map(|m|{let details=if m.role=="tool"{Some(json!({"name":m.extra.as_ref().and_then(|e|e["name"].as_str()),"result":serde_json::from_str::<Value>(&m.content).ok()}))}else{None};json!({"role":m.role,"content":m.content,"status":m.status,"turnId":m.turn_id,"details":details})}).collect();
    Ok(json!({"conversation":conversation,"messages":messages}))
}).await}
#[tauri::command]
pub async fn ai_conversation_delete(id:String)->Result<(),AiError>{let service=service()?;service.cancel_conversation(&id);service.database(move|r|HistoryStore::new(&r.store).delete(&id)).await}
#[tauri::command]
pub async fn ai_message_send(window:tauri::WebviewWindow,id:String,text:String)->Result<String,AiError>{
    service()?.send(id,text,Arc::new(move|event|{let _=window.emit("ai-diagnostic-event",event);})).await
}
#[tauri::command]
pub async fn ai_turn_cancel(id:String)->Result<(),AiError>{let service=service()?;service.cancel_turn(&id);service.database(move|r|HistoryStore::new(&r.store).revoke_turn(&id)).await}
