import { invoke, confirmAction, errorText } from "./api.js";
import { selectedRecordings } from "./recordings.js";
import { createAiController } from "./ai-state.js";
const names = { openai: "OpenAI", anthropic: "Claude", deepseek: "DeepSeek" };
const el = (tag, text, className) => {
  const node = document.createElement(tag);
  if (text !== undefined) node.textContent = text;
  if (className) node.className = className;
  return node;
};
window.addEventListener("DOMContentLoaded", () => {
  const messages = document.getElementById("ai-messages"),
    history = document.getElementById("ai-history"),
    composer = document.getElementById("ai-message"),
    send = document.getElementById("ai-send"),
    cancel = document.getElementById("ai-cancel"),
    feedback = document.getElementById("ai-chat-feedback");
  let historySignature = "";
  function retrieval(name, details) {
    const box = el("details", undefined, "ai-retrieval");
    box.append(
      el("summary", `Data read: ${name || "recording"}`),
      el("pre", JSON.stringify(details, null, 2)),
    );
    return box;
  }
  const controller = createAiController({
    invoke,
    subscribe: (callback) =>
      window.__TAURI__.event.listen("ai-diagnostic-event", (event) =>
        callback(event.payload),
      ),
    render(state) {
      const selection =
        state.conversation?.selection || state.preferences.defaultSelection;
      document.getElementById("ai-conversation-meta").textContent = selection
        ? `${names[selection.provider]} · ${selection.modelId} · ${state.conversation?.recordingIds.length ?? selectedRecordings.size} selected recording(s)`
        : "Choose a provider, API key and model in Settings to begin.";
      send.disabled = !controller.canSend;
      cancel.disabled = !state.busy || !state.activeTurn;
      feedback.textContent =
        state.error ||
        (!state.preferences.enabled
          ? "AI is optional and currently disabled. Enable it in Settings to begin."
          : !controller.canSend && !state.busy
            ? "Configure a key and test your chosen model in Settings."
            : "");
      document.getElementById("ai-usage").textContent = state.usage
        ? `${state.usage.input_tokens} input / ${state.usage.output_tokens} output tokens`
        : state.busy
          ? "Investigating…"
          : "";
      const nearBottom =
        messages.scrollHeight - messages.scrollTop - messages.clientHeight < 80;
      messages.replaceChildren();
      if (!state.messages.length && !state.busy) {
        const empty = el("div", undefined, "ai-empty");
        empty.append(
          el("h3", "Describe what is happening"),
          el(
            "p",
            "Tell the assistant about symptoms, when they occur, and recent repairs. Select saved recordings in Recordings; the assistant can retrieve relevant data as needed.",
          ),
          el(
            "p",
            "You can also ask without a recording. The assistant will explain when scanner evidence is missing.",
          ),
        );
        messages.append(empty);
      }
      for (const message of state.messages) {
        if (message.role === "tool") {
          messages.append(
            retrieval(message.details?.name, message.details?.result),
          );
          continue;
        }
        if (!["user", "assistant"].includes(message.role) || !message.content)
          continue;
        const bubble = el(
          "article",
          undefined,
          `ai-message ai-${message.role}`,
        );
        bubble.append(
          el("strong", message.role === "user" ? "You" : "Assistant"),
          el("div", message.content, "ai-message-text"),
        );
        if (
          ["cancelled", "interrupted", "error", "limit"].includes(
            message.status,
          )
        )
          bubble.append(el("small", `Turn ${message.status}`));
        messages.append(bubble);
      }
      if (state.busy) {
        const bubble = el("article", undefined, "ai-message ai-assistant");
        bubble.append(
          el("strong", "Assistant"),
          el(
            "div",
            state.stream || "Reviewing your question…",
            "ai-message-text",
          ),
        );
        messages.append(bubble);
        for (const activity of state.activity)
          messages.append(retrieval(activity.name, activity.details));
      }
      if (nearBottom) messages.scrollTop = messages.scrollHeight;
      const signature =
        JSON.stringify(state.conversations) + state.historyHasMore;
      if (signature !== historySignature) {
        historySignature = signature;
        history.replaceChildren();
        for (const chat of state.conversations) {
          const row = el("div", undefined, "ai-history-row");
          const open = el("button", chat.title || "Diagnostic conversation");
          open.title = `${names[chat.selection.provider]} · ${new Date(chat.createdUtcMs).toLocaleString()}`;
          open.addEventListener("click", () => controller.open(chat.id));
          const remove = el("button", "×");
          remove.setAttribute(
            "aria-label",
            `Delete ${chat.title || "conversation"}`,
          );
          remove.addEventListener("click", async () => {
            if (
              await confirmAction(
                "Delete this conversation from local history? Its recordings will remain.",
              )
            )
              controller.deleteConversation(chat.id);
          });
          row.append(open, remove);
          history.append(row);
        }
        if (state.historyHasMore) {
          const more = el("button", "Load older conversations");
          more.addEventListener("click", () => controller.loadMoreHistory());
          history.append(more);
        }
      }
    },
  });
  async function submit() {
    const text = composer.value.trim();
    if (!text || !controller.canSend) return;
    const selection =
      controller.state.conversation?.selection ||
      controller.state.preferences.defaultSelection;
    if (!selection) return;
    if (
      !controller.state.preferences.disclosedProviders?.includes(
        selection.provider,
      )
    ) {
      if (
        !(await confirmAction(
          `Send your messages and retrieved vehicle data to ${names[selection.provider]}? API usage may incur charges. Full VIN, device identifiers and local paths are excluded from automatic context; free text is sent as typed. Recording stays local until data is retrieved for a conversation.`,
        ))
      )
        return;
      try {
        await invoke("ai_disclosure_accept", { provider: selection.provider });
        await controller.refreshPreferences();
      } catch (e) {
        feedback.textContent = errorText(e);
        return;
      }
    }
    if (!controller.state.conversation) {
      const chat = await controller.createConversation({
        selection,
        recordingIds: [...selectedRecordings],
        profile: text,
      });
      if (!chat) return;
    }
    composer.value = "";
    await controller.send(text);
  }
  send.addEventListener("click", submit);
  composer.addEventListener("keydown", (event) => {
    if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      submit();
    }
  });
  cancel.addEventListener("click", () => controller.cancel());
  document
    .getElementById("ai-new-chat")
    .addEventListener("click", () => controller.newConversation());
  document
    .getElementById("ai-open-settings")
    .addEventListener("click", () =>
      document.querySelector('[data-screen="settings"]').click(),
    );
  window.addEventListener("ai-settings-changed", () =>
    controller.refreshPreferences(),
  );
  window.addEventListener("recording-selection-changed", async () => {
    await controller.refreshHistory();
    if (!controller.state.conversation) controller.newConversation();
  });
  window.addEventListener("beforeunload", () => controller.dispose());
  controller.initialize();
});
