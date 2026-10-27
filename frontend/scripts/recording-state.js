export function createRecordingController({invoke,render}) {
  const state={sessions:[],status:{recording:false},busy:false,error:'',selectedId:null,preview:null};
  const notify=()=>render(state);
  async function refresh(){
    const [sessions,status]=await Promise.all([invoke('recording_list',{limit:100,offset:0}),invoke('recording_status')]);
    state.sessions=sessions;state.status=status;notify();
  }
  async function mutate(work){
    if(state.busy)return false;
    state.busy=true;state.error='';notify();
    try{await work();await refresh();return true;}catch(error){state.error=typeof error==='string'?error:error?.message||'Recording request failed.';return false;}
    finally{state.busy=false;notify();}
  }
  return {state,
    async initialize(){try{await refresh();}catch(error){state.error=error?.message||'Recording history could not be loaded.';notify();}},
    async refresh(){if(state.busy)return;try{await refresh();}catch(error){state.error=error?.message||'Recording status could not be refreshed.';notify();}},
    start:options=>mutate(async()=>{const session=await invoke('recording_start',options);state.selectedId=session.id;}),
    stop:()=>mutate(()=>invoke('recording_stop')),
    select(id){state.selectedId=id;notify();},
    async previewDelete(id){state.preview=await invoke('recording_delete_preview',{id});return state.preview;},
    async delete(id,token){if(state.preview?.recordingId!==id||state.preview.revisionToken!==token)return false;return mutate(async()=>{await invoke('recording_delete',{id,revisionToken:token});state.preview=null;});},
    dispose(){},
  };
}
