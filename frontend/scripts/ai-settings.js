import {invoke,confirmAction,errorText} from './api.js';
const labels={openai:'OpenAI',anthropic:'Claude',deepseek:'DeepSeek'};
window.addEventListener('DOMContentLoaded',()=>{
  const provider=document.getElementById('ai-provider'),enabled=document.getElementById('ai-enabled'),key=document.getElementById('ai-key'),model=document.getElementById('ai-model-id'),models=document.getElementById('ai-model-list'),feedback=document.getElementById('ai-settings-feedback');
  let preferences={enabled:false,selections:{},models:{},validatedModels:{}},status={configured:false},busy=false;
  function render(){
    enabled.checked=preferences.enabled;
    document.getElementById('ai-key-status').textContent=status.error||(status.configured?`Key configured · ${status.persistence==='session'?'this app session only':'operating system credential store'}`:'No API key configured');
    for(const id of ['ai-save-key','ai-remove-key','ai-save-model'])document.getElementById(id).disabled=busy;
    document.getElementById('ai-refresh-models').disabled=busy||!preferences.enabled||!status.configured;
    document.getElementById('ai-test-model').disabled=busy||!preferences.enabled||!status.configured;
    provider.disabled=busy;
    const chosen=model.value;models.replaceChildren(new Option('Choose a discovered model',''));
    for(const item of preferences.models?.[provider.value]||[])models.append(new Option(item.label||item.id,item.id));models.value=chosen;
    document.getElementById('ai-model-status').textContent=preferences.validatedModels?.[provider.value]?.includes(chosen)?'Tool compatibility verified':'Test the selected model before diagnosing.';
  }
  async function load(){preferences=await invoke('ai_preferences_get');status=await invoke('ai_key_status',{provider:provider.value});render();window.dispatchEvent(new Event('ai-settings-changed'));}
  async function saveSelection(){
    const id=model.value.trim();const selections={...preferences.selections};if(id)selections[provider.value]=id;
    preferences=await invoke('ai_preferences_set',{update:{enabled:enabled.checked,selections,defaultSelection:id?{provider:provider.value,modelId:id}:preferences.defaultSelection||null}});
  }
  async function run(work,message){if(busy)return;busy=true;feedback.textContent='Working…';render();try{await work();feedback.textContent=message;}catch(e){feedback.textContent=errorText(e);}finally{busy=false;try{await load();}catch(e){feedback.textContent=errorText(e);}render();}}
  enabled.addEventListener('change',async()=>{try{preferences=await invoke('ai_preferences_set',{update:{enabled:enabled.checked,defaultSelection:preferences.defaultSelection||null,selections:preferences.selections||{}}});feedback.textContent=preferences.enabled?'AI enabled. Provider requests run only when you test or send a message.':'AI disabled. Active provider requests were cancelled.';window.dispatchEvent(new Event('ai-settings-changed'));render();}catch(e){feedback.textContent=errorText(e);}});
  provider.addEventListener('change',async()=>{model.value=preferences.selections?.[provider.value]||'';try{status=await invoke('ai_key_status',{provider:provider.value});feedback.textContent='';render();}catch(e){feedback.textContent=errorText(e);}});
  models.addEventListener('change',()=>{if(models.value)model.value=models.value;render();});model.addEventListener('input',()=>{document.getElementById('ai-model-status').textContent=preferences.validatedModels?.[provider.value]?.includes(model.value.trim())?'Tool compatibility verified':'Test the selected model before diagnosing.';});
  document.getElementById('ai-save-key').addEventListener('click',()=>run(async()=>{const secret=key.value;key.value='';await invoke('ai_key_set',{provider:provider.value,secret,persistence:document.getElementById('ai-session-key').checked?'session':'native'});},'API key saved. Test the model to verify access.'));
  document.getElementById('ai-remove-key').addEventListener('click',()=>run(()=>invoke('ai_key_remove',{provider:provider.value}),'API key removed.'));
  document.getElementById('ai-refresh-models').addEventListener('click',()=>run(()=>invoke('ai_models_refresh',{provider:provider.value}),'Available models refreshed.'));
  document.getElementById('ai-save-model').addEventListener('click',()=>run(saveSelection,'Model preference saved. Existing conversations keep their provider and model.'));
  document.getElementById('ai-test-model').addEventListener('click',async()=>{
    if(!model.value.trim()){feedback.textContent='Choose or enter a model ID first.';return;}
    if(!await confirmAction(`Test ${model.value.trim()} with ${labels[provider.value]}? This sends only a small synthetic tool request and may incur an API charge. No vehicle data is sent.`))return;
    run(async()=>{await invoke('ai_model_test',{provider:provider.value,modelId:model.value.trim()});await saveSelection();},'Tool compatibility verified and model preference saved.');
  });
  (async()=>{try{preferences=await invoke('ai_preferences_get');provider.value=preferences.defaultSelection?.provider||'openai';model.value=preferences.selections?.[provider.value]||preferences.defaultSelection?.modelId||'';await load();}catch(e){feedback.textContent=errorText(e);}})();
});
