import test from "node:test";
import assert from "node:assert/strict";
import { createRecordingController } from "../scripts/recording-state.js";

test("manual recording prevents duplicate starts and never calls an AI provider", async () => {
  const calls = [];
  let recording = false;
  const invoke = async (command) => {
    calls.push(command);
    if (command === "recording_start") {
      recording = true;
      return { id: "one" };
    }
    if (command === "recording_status") return { recording };
    if (command === "recording_list") return [{ id: "one" }];
  };
  const controller = createRecordingController({ invoke, render() {} });
  await controller.initialize();
  await Promise.all([
    controller.start({ intervalSeconds: 1, label: "Test" }),
    controller.start({ intervalSeconds: 1, label: "Test" }),
  ]);
  assert.equal(calls.filter((x) => x === "recording_start").length, 1);
  assert.equal(controller.state.status.recording, true);
  assert.ok(calls.every((x) => x.startsWith("recording_")));
});

test("saved sessions restore and deletion requires the reviewed preview", async () => {
  let deleted = false;
  const controller = createRecordingController({
    invoke: async (command) => {
      if (command === "recording_list")
        return [{ id: "saved", label: "Saved" }];
      if (command === "recording_status") return { recording: false };
      if (command === "recording_delete_preview")
        return {
          recordingId: "saved",
          conversationIds: [],
          revisionToken: "reviewed",
        };
      if (command === "recording_delete") deleted = true;
    },
    render() {},
  });
  await controller.initialize();
  assert.equal(controller.state.sessions[0].id, "saved");
  assert.equal(await controller.delete("saved", "unreviewed"), false);
  assert.equal(deleted, false);
  await controller.previewDelete("saved");
  await controller.delete("saved", "reviewed");
  assert.equal(deleted, true);
});
test("older recording pages remain available after status refresh", async () => {
  const recordings = Array.from({ length: 201 }, (_, i) => ({
    id: `saved-${i}`,
  }));
  const controller = createRecordingController({
    invoke: async (command, args) =>
      command === "recording_list"
        ? recordings.slice(args.offset, args.offset + args.limit)
        : { recording: false },
    render() {},
  });
  await controller.initialize();
  assert.equal(controller.state.sessions.length, 100);
  await controller.loadMore();
  assert.equal(controller.state.sessions.length, 200);
  await controller.refresh();
  assert.equal(controller.state.sessions.length, 200);
});
