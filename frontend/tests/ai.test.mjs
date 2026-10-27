import test from "node:test";
import assert from "node:assert/strict";
import { createAiController } from "../scripts/ai-state.js";
function fixture(configured = true, sendGate = null) {
  let handler;
  const calls = [];
  const selection = { provider: "openai", modelId: "fixture" };
  const preferences = {
    enabled: true,
    defaultSelection: selection,
    validatedModels: { openai: ["fixture"] },
    disclosedProviders: ["openai"],
  };
  const chat = {
    id: "chat1",
    selection,
    recordingIds: ["recording1"],
    profile: "Warm idle",
    status: "ready",
    activeTurn: null,
  };
  const chats = [chat];
  const invoke = async (command, args) => {
    calls.push({ command, args });
    if (command === "ai_preferences_get") return preferences;
    if (command === "ai_key_status") return { configured };
    if (command === "ai_conversation_list") return [...chats];
    if (command === "ai_conversation_get") {
      if (!chats.length) throw { message: "Deleted" };
      return { conversation: chat, messages: [] };
    }
    if (command === "ai_conversation_create") return chat;
    if (command === "ai_message_send")
      return sendGate ? await sendGate : "turn1";
  };
  return {
    controller: createAiController({
      invoke,
      subscribe: async (callback) => {
        handler = callback;
        return () => {};
      },
      render() {},
    }),
    emit: (event) => handler(event),
    calls,
    chat,
    chats,
    preferences,
  };
}
test("sending requires a configured key and tested model", async () => {
  const f = fixture(false);
  await f.controller.initialize();
  assert.equal(f.controller.canSend, false);
});
test("a saved chat keeps its provider and ignores another turns output", async () => {
  const f = fixture();
  await f.controller.initialize();
  await f.controller.open("chat1");
  await f.controller.send("Why does it idle roughly?");
  f.preferences.defaultSelection = { provider: "deepseek", modelId: "other" };
  f.emit({
    conversationId: "chat1",
    turnId: "old-turn",
    kind: "text_delta",
    text: "stale",
  });
  f.emit({
    conversationId: "chat1",
    turnId: "turn1",
    kind: "text_delta",
    text: "<script>unsafe()</script>",
  });
  assert.equal(f.controller.state.stream, "<script>unsafe()</script>");
  assert.equal(f.controller.state.conversation.selection.provider, "openai");
  assert.ok(
    f.calls.every(
      (call) => !JSON.stringify(call.args || {}).includes("api-key"),
    ),
  );
});
test("old output is ignored while a new send awaits its turn ID", async () => {
  let resolve;
  const gate = new Promise((r) => {
    resolve = r;
  });
  const f = fixture(true, gate);
  await f.controller.initialize();
  await f.controller.open("chat1");
  const sending = f.controller.send("New question");
  f.emit({
    conversationId: "chat1",
    turnId: "old-turn",
    kind: "text_delta",
    text: "old",
  });
  assert.equal(f.controller.state.stream, "");
  resolve("turn1");
  await sending;
});
test("deleting a recording clears its removed conversation from the current view", async () => {
  const f = fixture();
  await f.controller.initialize();
  await f.controller.open("chat1");
  f.chats.length = 0;
  await f.controller.refreshHistory();
  assert.equal(f.controller.state.conversation, null);
  assert.deepEqual(f.controller.state.messages, []);
});
test("switching to a running chat releases the previous pending request identity", async () => {
  let handler, resolveSend;
  const gate = new Promise((resolve) => {
    resolveSend = resolve;
  });
  const selection = { provider: "openai", modelId: "fixture" };
  const a = { id: "a", selection, status: "ready", activeTurn: null },
    b = { id: "b", selection, status: "running", activeTurn: "bt" };
  const controller = createAiController({
    invoke: async (command, args) => {
      if (command === "ai_preferences_get")
        return {
          enabled: true,
          defaultSelection: selection,
          validatedModels: { openai: ["fixture"] },
        };
      if (command === "ai_key_status") return { configured: true };
      if (command === "ai_conversation_list") return [a, b];
      if (command === "ai_conversation_get")
        return { conversation: args.id === "a" ? a : b, messages: [] };
      if (command === "ai_message_send") return await gate;
    },
    subscribe: async (fn) => {
      handler = fn;
      return () => {};
    },
    render() {},
  });
  await controller.initialize();
  await controller.open("a");
  const sending = controller.send("Question A");
  await controller.open("b");
  resolveSend("at");
  await sending;
  b.status = "completed";
  await handler({
    conversationId: "b",
    turnId: "bt",
    requestId: "request-b",
    kind: "completed",
    text: "",
  });
  assert.equal(controller.state.conversation.id, "b");
  assert.equal(controller.state.busy, false);
});
