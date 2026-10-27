export function createRecordingController({ invoke, render }) {
  const state = {
    sessions: [],
    status: { recording: false },
    busy: false,
    error: "",
    selectedId: null,
    preview: null,
    hasMore: false,
  };
  let pageCount = 1;
  const notify = () => render(state);
  async function refresh() {
    const [pages, status] = await Promise.all([
      Promise.all(
        Array.from({ length: pageCount }, (_, index) =>
          invoke("recording_list", { limit: 100, offset: index * 100 }),
        ),
      ),
      invoke("recording_status"),
    ]);
    state.sessions = [...new Map(pages.flat().map((s) => [s.id, s])).values()];
    state.hasMore = pages.at(-1).length === 100;
    state.status = status;
    notify();
  }
  async function mutate(work) {
    if (state.busy) return false;
    state.busy = true;
    state.error = "";
    notify();
    try {
      await work();
      await refresh();
      return true;
    } catch (error) {
      state.error =
        typeof error === "string"
          ? error
          : error?.message || "Recording request failed.";
      return false;
    } finally {
      state.busy = false;
      notify();
    }
  }
  return {
    state,
    async initialize() {
      try {
        await refresh();
      } catch (error) {
        state.error =
          error?.message || "Recording history could not be loaded.";
        notify();
      }
    },
    async refresh() {
      if (state.busy) return;
      try {
        await refresh();
      } catch (error) {
        state.error =
          error?.message || "Recording status could not be refreshed.";
        notify();
      }
    },
    start: (options) =>
      mutate(async () => {
        const session = await invoke("recording_start", options);
        state.selectedId = session.id;
      }),
    loadMore: () =>
      mutate(async () => {
        pageCount += 1;
      }),
    stop: () => mutate(() => invoke("recording_stop")),
    select(id) {
      state.selectedId = id;
      notify();
    },
    async previewDelete(id) {
      state.preview = await invoke("recording_delete_preview", { id });
      return state.preview;
    },
    async delete(id, token) {
      if (
        state.preview?.recordingId !== id ||
        state.preview.revisionToken !== token
      )
        return false;
      return mutate(async () => {
        await invoke("recording_delete", { id, revisionToken: token });
        state.preview = null;
      });
    },
    dispose() {},
  };
}
