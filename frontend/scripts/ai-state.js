export function createAiController({invoke,subscribe,render}){
  const state={preferences:{enabled:false},keys:{},conversations:[],conversation:null,messages:[],busy:false,activeTurn:null,stream:'',activity:[],error:'',usage:null};
  let unlisten=()=>{};
  const notify=()=>render(state);
  const errorText=e=>typeof e==='string'?e:e?.message||'AI request failed.';
  async function refreshPreferences(){
    state.preferences=await invoke('ai_preferences_get');
    await Promise.all(['openai','anthropic','deepseek'].map(async provider=>{state.keys[provider]=await invoke('ai_key_status',{provider});}));notify();
  }
  async function reload(id){
    const result=await invoke('ai_conversation_get',{id});if(state.conversation?.id!==id)return;
    state.conversation=result.conversation;state.messages=result.messages;notify();
  }
  async function receive(event){
    if(!state.busy||event.conversationId!==state.conversation?.id)return;
    if(state.activeTurn&&event.turnId!==state.activeTurn)return;
    state.activeTurn=event.turnId;
    if(event.kind==='text_delta')state.stream+=event.text;
    if(event.kind==='usage')state.usage=event.details;
    if(event.kind==='tool_activity'){state.activity.push({name:event.text,details:event.details});if(state.stream&&!state.stream.endsWith('\n\n'))state.stream+='\n\n';}
    if(['completed','limit','error','cancelled'].includes(event.kind)){
      state.busy=false;state.activeTurn=null;state.error=event.text||'';state.stream='';
      try{await reload(event.conversationId);state.conversations=await invoke('ai_conversation_list');}catch(e){state.error=errorText(e);}
    }notify();
  }
  const controller={state,
    get canSend(){const selection=state.conversation?.selection||state.preferences.defaultSelection;return Boolean(state.preferences.enabled&&selection&&state.keys[selection.provider]?.configured&&state.preferences.validatedModels?.[selection.provider]?.includes(selection.modelId)&&!state.busy);},
    async initialize(){try{unlisten=await subscribe(receive);await refreshPreferences();state.conversations=await invoke('ai_conversation_list');}catch(e){state.error=errorText(e);}notify();},
    async refreshPreferences(){try{await refreshPreferences();}catch(e){state.error=errorText(e);notify();}},
    async open(id){try{const result=await invoke('ai_conversation_get',{id});state.conversation=result.conversation;state.messages=result.messages;state.busy=result.conversation.status==='running';state.activeTurn=result.conversation.activeTurn||null;state.stream='';state.activity=[];state.error='';}catch(e){state.error=errorText(e);}notify();},
    async createConversation(selection){try{state.conversation=await invoke('ai_conversation_create',selection);state.messages=[];state.stream='';state.activity=[];state.busy=false;state.activeTurn=null;state.conversations=await invoke('ai_conversation_list');state.error='';notify();return state.conversation;}catch(e){state.error=errorText(e);notify();return null;}},
    newConversation(){state.conversation=null;state.messages=[];state.stream='';state.activity=[];state.busy=false;state.activeTurn=null;state.error='';state.usage=null;notify();},
    async send(text){
      if(!controller.canSend||!state.conversation||!text.trim())return false;
      const id=state.conversation.id;state.busy=true;state.activeTurn=null;state.stream='';state.activity=[];state.error='';state.usage=null;
      state.messages.push({role:'user',content:text,status:'pending'});notify();
      try{const turnId=await invoke('ai_message_send',{id,text});if(state.conversation?.id===id&&state.busy)state.activeTurn=turnId;return true;}
      catch(e){if(state.conversation?.id===id){state.busy=false;state.activeTurn=null;state.error=errorText(e);try{await reload(id);}catch{}}notify();return false;}
    },
    async cancel(){if(!state.activeTurn)return;try{await invoke('ai_turn_cancel',{id:state.activeTurn});}catch(e){state.error=errorText(e);notify();}},
    async deleteConversation(id){try{await invoke('ai_conversation_delete',{id});if(state.conversation?.id===id)controller.newConversation();state.conversations=await invoke('ai_conversation_list');notify();return true;}catch(e){state.error=errorText(e);notify();return false;}},
    dispose(){unlisten();},
  };return controller;
}
