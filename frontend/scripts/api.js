export const invoke = (command, args = {}) =>
  window.__TAURI__.core.invoke(command, args);
export const errorText = (error) =>
  typeof error === "string"
    ? error
    : error?.message || "The request could not be completed.";
export const confirmAction = (message) =>
  window.__TAURI__?.dialog?.confirm
    ? window.__TAURI__.dialog.confirm(message, {
        title: "OBDium",
        kind: "warning",
      })
    : Promise.resolve(window.confirm(message));
