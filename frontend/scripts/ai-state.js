let requestSequence = 0;
export function createAiController({ invoke, subscribe, render }) {
  const state = {
    preferences: { enabled: false },
    keys: {},
    conversations: [],
    conversation: null,
    messages: [],
    busy: false,
    activeTurn: null,
    stream: "",
    activity: [],
    error: "",
    usage: null,
  };
  let unlisten = () => {},
    pendingRequest = null,
    historyPages = 1,
    viewRevision = 0,
    historyRevision = 0;
  async function loadHistory() {
    const revision = ++historyRevision;
    const pages = await Promise.all(
      Array.from({ length: historyPages }, (_, index) =>
        invoke("ai_conversation_list", { limit: 100, offset: index * 100 }),
      ),
    );
    if (revision !== historyRevision) return;
    state.conversations = [
      ...new Map(pages.flat().map((chat) => [chat.id, chat])).values(),
    ];
    state.historyHasMore = pages.at(-1).length === 100;
  }
  const notify = () => render(state);
  const errorText = (e) =>
    typeof e === "string" ? e : e?.message || "AI request failed.";
  async function refreshPreferences() {
    state.preferences = await invoke("ai_preferences_get");
    await Promise.all(
      ["openai", "anthropic", "deepseek"].map(async (provider) => {
        state.keys[provider] = await invoke("ai_key_status", { provider });
      }),
    );
    notify();
  }
  async function reload(id, revision = viewRevision) {
    const result = await invoke("ai_conversation_get", { id });
    if (state.conversation?.id !== id || revision !== viewRevision) return;
    state.conversation = result.conversation;
    state.messages = result.messages;
    notify();
  }
  async function receive(event) {
    const revision = viewRevision;
    if (!state.busy || event.conversationId !== state.conversation?.id) return;
    if (pendingRequest && event.requestId !== pendingRequest) return;
    if (state.activeTurn && event.turnId !== state.activeTurn) return;
    state.activeTurn = event.turnId;
    if (event.kind === "text_delta") state.stream += event.text;
    if (event.kind === "usage") state.usage = event.details;
    if (event.kind === "tool_activity") {
      state.activity.push({ name: event.text, details: event.details });
      if (state.stream && !state.stream.endsWith("\n\n"))
        state.stream += "\n\n";
    }
    if (["completed", "limit", "error", "cancelled"].includes(event.kind)) {
      state.busy = false;
      state.activeTurn = null;
      pendingRequest = null;
      state.error = event.text || "";
      state.stream = "";
      try {
        await reload(event.conversationId, revision);
        if (revision !== viewRevision) return;
        await loadHistory();
      } catch (e) {
        if (revision === viewRevision) state.error = errorText(e);
      }
    }
    notify();
  }
  const controller = {
    state,
    get canSend() {
      const selection =
        state.conversation?.selection || state.preferences.defaultSelection;
      return Boolean(
        state.preferences.enabled &&
        selection &&
        state.keys[selection.provider]?.configured &&
        state.preferences.validatedModels?.[selection.provider]?.includes(
          selection.modelId,
        ) &&
        !state.busy,
      );
    },
    async initialize() {
      try {
        unlisten = await subscribe(receive);
        await refreshPreferences();
        await loadHistory();
      } catch (e) {
        state.error = errorText(e);
      }
      notify();
    },
    async refreshHistory() {
      try {
        await loadHistory();
        if (
          state.conversation &&
          !state.conversations.some((c) => c.id === state.conversation.id)
        ) {
          try {
            await reload(state.conversation.id);
          } catch {
            controller.newConversation();
          }
        }
      } catch (e) {
        state.error = errorText(e);
      }
      notify();
    },
    async loadMoreHistory() {
      historyPages += 1;
      await controller.refreshHistory();
    },
    async refreshPreferences() {
      try {
        await refreshPreferences();
      } catch (e) {
        state.error = errorText(e);
        notify();
      }
    },
    async open(id) {
      const revision = ++viewRevision;
      pendingRequest = null;
      try {
        const result = await invoke("ai_conversation_get", { id });
        if (revision !== viewRevision) return;
        state.conversation = result.conversation;
        state.messages = result.messages;
        state.busy = result.conversation.status === "running";
        state.activeTurn = result.conversation.activeTurn || null;
        state.stream = "";
        state.activity = [];
        state.error = "";
      } catch (e) {
        if (revision === viewRevision) state.error = errorText(e);
      }
      notify();
    },
    async createConversation(selection) {
      const revision = ++viewRevision;
      pendingRequest = null;
      try {
        const conversation = await invoke("ai_conversation_create", selection);
        if (revision !== viewRevision) return null;
        state.conversation = conversation;
        state.messages = [];
        state.stream = "";
        state.activity = [];
        state.busy = false;
        state.activeTurn = null;
        await loadHistory();
        if (revision !== viewRevision) return null;
        state.error = "";
        notify();
        return conversation;
      } catch (e) {
        if (revision === viewRevision) state.error = errorText(e);
        notify();
        return null;
      }
    },
    newConversation() {
      viewRevision += 1;
      pendingRequest = null;
      state.conversation = null;
      state.messages = [];
      state.stream = "";
      state.activity = [];
      state.busy = false;
      state.activeTurn = null;
      state.error = "";
      state.usage = null;
      notify();
    },
    async send(text) {
      if (!controller.canSend || !state.conversation || !text.trim())
        return false;
      const id = state.conversation.id;
      const revision = viewRevision;
      state.busy = true;
      state.activeTurn = null;
      state.stream = "";
      state.activity = [];
      state.error = "";
      state.usage = null;
      const requestId =
        globalThis.crypto?.randomUUID?.() ||
        `${Date.now()}-${++requestSequence}`;
      pendingRequest = requestId;
      state.messages.push({ role: "user", content: text, status: "pending" });
      notify();
      try {
        const turnId = await invoke("ai_message_send", { id, text, requestId });
        if (
          revision === viewRevision &&
          state.conversation?.id === id &&
          state.busy &&
          pendingRequest === requestId
        ) {
          state.activeTurn = turnId;
          pendingRequest = null;
        }
        return true;
      } catch (e) {
        if (
          revision === viewRevision &&
          state.conversation?.id === id &&
          pendingRequest === requestId
        ) {
          state.busy = false;
          state.activeTurn = null;
          pendingRequest = null;
          state.error = errorText(e);
          try {
            await reload(id);
          } catch {}
        }
        notify();
        return false;
      }
    },
    async cancel() {
      if (!state.activeTurn) return;
      try {
        await invoke("ai_turn_cancel", { id: state.activeTurn });
      } catch (e) {
        state.error = errorText(e);
        notify();
      }
    },
    async deleteConversation(id) {
      try {
        await invoke("ai_conversation_delete", { id });
        if (state.conversation?.id === id) controller.newConversation();
        state.conversations = await invoke("ai_conversation_list");
        notify();
        return true;
      } catch (e) {
        state.error = errorText(e);
        notify();
        return false;
      }
    },
    dispose() {
      unlisten();
    },
  };
  return controller;
}
