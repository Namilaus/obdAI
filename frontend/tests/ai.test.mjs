import test from 'node:test';
import assert from 'node:assert/strict';
import {createAiController} from '../scripts/ai-state.js';
function fixture(configured=true){
 let handler;const calls=[];const selection={provider:'openai',modelId:'fixture'};
 const preferences={enabled:true,defaultSelection:selection,validatedModels:{openai:['fixture']},disclosedProviders:['openai']};
 const chat={id:'chat1',selection,recordingIds:['recording1'],profile:'Warm idle',status:'ready',activeTurn:null};
 const invoke=async(command,args)=>{calls.push({command,args});if(command==='ai_preferences_get')return preferences;if(command==='ai_key_status')return {configured};if(command==='ai_conversation_list')return [chat];if(command==='ai_conversation_get')return {conversation:chat,messages:[]};if(command==='ai_conversation_create')return chat;if(command==='ai_message_send')return 'turn1';};
 return {controller:createAiController({invoke,subscribe:async callback=>{handler=callback;return()=>{};},render(){}}),emit:event=>handler(event),calls,chat,preferences};
}
test('sending requires a configured key and tested model',async()=>{const f=fixture(false);await f.controller.initialize();assert.equal(f.controller.canSend,false);});
test('a saved chat keeps its provider and ignores another turns output',async()=>{
 const f=fixture();await f.controller.initialize();await f.controller.open('chat1');await f.controller.send('Why does it idle roughly?');
 f.preferences.defaultSelection={provider:'deepseek',modelId:'other'};
 f.emit({conversationId:'chat1',turnId:'old-turn',kind:'text_delta',text:'stale'});
 f.emit({conversationId:'chat1',turnId:'turn1',kind:'text_delta',text:'<script>unsafe()</script>'});
 assert.equal(f.controller.state.stream,'<script>unsafe()</script>');
 assert.equal(f.controller.state.conversation.selection.provider,'openai');
 assert.ok(f.calls.every(call=>!JSON.stringify(call.args||{}).includes('api-key')));
});
