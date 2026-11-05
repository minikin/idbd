import assert from "node:assert/strict";
import { test } from "node:test";

import { advanceReplay } from "../src/replay.js";

// An observable stream with a one-entry queue: a replay must work even with a
// tighter pending-change budget than Rust's, without dropping or moving changes.
class Explorer {
  sample = 0;
  pending = null;
  changes = [];
  total = 0;

  completed() { return this.sample; }
  hasFailed() { return false; }
  scheduleChange(sample) {
    assert.equal(this.pending, null, "only one change waits at a time");
    assert.ok(sample > this.sample);
    this.pending = sample;
  }
  run(samples) {
    for (let i = 0; i < samples; i += 1) {
      this.sample += 1;
      if (this.pending === this.sample) {
        this.changes.push(this.sample);
        this.pending = null;
      }
      this.total += this.changes.length * this.sample;
    }
  }
}

for (const batch of [1, 256, 20000]) {
  test(`replays 10001 changes with batch size ${batch}`, () => {
    const changes = Array.from({ length: 10001 }, (_, i) => i + 1);
    const plan = { changes, nextChange: 0 };
    const original = new Explorer();
    for (const sample of changes) {
      original.scheduleChange(sample);
      original.run(1);
    }
    const replay = new Explorer();
    while (replay.completed() < changes.length) {
      advanceReplay(replay, plan, Math.min(batch, changes.length - replay.completed()));
    }
    assert.deepEqual(replay, original);
    assert.equal(plan.nextChange, changes.length);
  });
}

test("changes at batch boundaries apply before their recorded samples", () => {
  const explorer = new Explorer();
  const plan = { changes: [3, 4, 9], nextChange: 0 };
  advanceReplay(explorer, plan, 3);
  assert.deepEqual(explorer.changes, [3]);
  advanceReplay(explorer, plan, 5);
  assert.deepEqual(explorer.changes, [3, 4]);
  advanceReplay(explorer, plan, 1);
  assert.deepEqual(explorer.changes, [3, 4, 9]);
});

test("partial learner failure stops replay before scheduling more changes", () => {
  const explorer = new Explorer();
  explorer.hasFailed = () => explorer.completed() >= 2;
  explorer.run = (samples) => { explorer.sample += Math.min(samples, 2 - explorer.sample); };
  const plan = { changes: [3, 4], nextChange: 0 };
  advanceReplay(explorer, plan, 10);
  assert.equal(explorer.completed(), 2);
  assert.equal(plan.nextChange, 1);
  assert.equal(explorer.pending, 3);
});

test("a first-sample failure is propagated without advancing further", () => {
  const explorer = new Explorer();
  explorer.run = () => { throw new Error("learner failure"); };
  const plan = { changes: [3, 4], nextChange: 0 };
  assert.throws(() => advanceReplay(explorer, plan, 10), /learner failure/);
  assert.equal(explorer.completed(), 0);
  assert.equal(plan.nextChange, 1);
});
