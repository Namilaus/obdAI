import {invoke,confirmAction,errorText} from './api.js';
import {createRecordingController} from './recording-state.js';
export const selectedRecordings=new Set();
const el=(tag,text,className)=>{const node=document.createElement(tag);if(text!==undefined)node.textContent=text;if(className)node.className=className;return node;};

window.addEventListener('DOMContentLoaded',()=>{
  const list=document.getElementById('recording-list'),detail=document.getElementById('recording-detail');
  const start=document.getElementById('recording-start'),stop=document.getElementById('recording-stop'),interval=document.getElementById('recording-interval');
  let lastList='';
  const controller=createRecordingController({invoke,render(state){
    start.disabled=state.busy||state.status.recording;stop.disabled=state.busy||!state.status.recording;interval.disabled=state.status.recording;
    document.getElementById('recording-status').textContent=state.status.recording?`Recording · ${state.status.snapshotCount} saved snapshots`:'Recording stopped';
    document.getElementById('recording-status').classList.toggle('is-recording',state.status.recording);
    document.getElementById('recording-error').textContent=state.error||state.status.error||'';
    const fingerprint=JSON.stringify(state.sessions);
    if(fingerprint===lastList)return;lastList=fingerprint;list.replaceChildren();
    if(!state.sessions.length)list.append(el('p','No recordings yet. Connect your scanner, then choose Start recording.','recording-empty'));
    for(const session of state.sessions){
      const row=el('article',undefined,'recording-row');
      const checkbox=el('input');checkbox.type='checkbox';checkbox.checked=selectedRecordings.has(session.id);checkbox.setAttribute('aria-label',`Use ${session.label||'recording'} for diagnosis`);
      checkbox.addEventListener('change',()=>{checkbox.checked?selectedRecordings.add(session.id):selectedRecordings.delete(session.id);window.dispatchEvent(new Event('recording-selection-changed'));});
      const info=el('div',undefined,'recording-info');info.append(el('strong',session.label||'Diagnostic recording'),el('span',`${new Date(session.startedAt.utcMs).toLocaleString()} · ${session.source}${session.simulated?' / simulated':''} · every ${session.intervalSeconds}s`),el('span',`${session.snapshotCount} snapshots · ${session.reason||'recording'} · vehicle ${session.vehicleId.slice(0,8)}`));
      const inspect=el('button','Inspect');inspect.addEventListener('click',async()=>{controller.select(session.id);detail.replaceChildren(el('h3',session.label||'Recording'));try{const [summary,samples]=await Promise.all([invoke('recording_summary',{id:session.id}),invoke('recording_samples',{id:session.id,limit:50})]);const pre=el('pre',JSON.stringify({summary,samples},null,2));detail.append(pre);}catch(e){detail.append(el('p',errorText(e)));}});
      const assign=el('button','Vehicle / label');assign.addEventListener('click',()=>{
        detail.replaceChildren(el('h3','Assign this recording to a vehicle'));
        const label=el('input');label.value=session.label;label.maxLength=256;label.setAttribute('aria-label','Recording label');
        const vehicles=el('select');vehicles.setAttribute('aria-label','Vehicle identity');const known=new Map(state.sessions.map(s=>[s.vehicleId,s]));
        for(const [id,s]of known){const option=el('option',`${s.label||'Vehicle'} · ${id.slice(0,8)}`);option.value=id;option.selected=id===session.vehicleId;vehicles.append(option);}
        const fresh=el('option','A different vehicle');fresh.value='new';vehicles.append(fresh);
        const save=el('button','Save assignment');save.addEventListener('click',async()=>{try{await invoke('recording_assign_vehicle',{id:session.id,vehicleId:vehicles.value==='new'?crypto.randomUUID():vehicles.value,label:label.value});await controller.refresh();detail.replaceChildren(el('p','Vehicle assignment saved.'));}catch(e){detail.append(el('p',errorText(e)));}});
        detail.append(el('p','Only combine recordings when you know they belong to the same vehicle.'),label,vehicles,save);
      });assign.disabled=!session.endedAt;
      const remove=el('button','Delete');remove.disabled=!session.endedAt;remove.addEventListener('click',async()=>{try{const preview=await controller.previewDelete(session.id);if(await confirmAction(`Delete this recording and ${preview.conversationIds.length} linked chat(s) from local history? Data already sent to an AI provider cannot be recalled.`)){if(await controller.delete(session.id,preview.revisionToken)){selectedRecordings.delete(session.id);detail.replaceChildren();window.dispatchEvent(new Event('recording-selection-changed'));}}}catch(e){document.getElementById('recording-error').textContent=errorText(e);}});
      row.append(checkbox,info,inspect,assign,remove);list.append(row);
    }
  }});
  const savedInterval=localStorage.getItem('recordingInterval');if(['1','2','5','10'].includes(savedInterval))interval.value=savedInterval;
  interval.addEventListener('change',()=>localStorage.setItem('recordingInterval',interval.value));
  start.addEventListener('click',()=>controller.start({intervalSeconds:Number(interval.value),label:document.getElementById('recording-label').value.trim()||'Diagnostic recording'}));
  stop.addEventListener('click',()=>controller.stop());
  controller.initialize();const timer=setInterval(()=>controller.refresh(),2000);window.addEventListener('beforeunload',()=>{clearInterval(timer);controller.dispose();});
});
