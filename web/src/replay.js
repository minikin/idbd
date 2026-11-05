/**
 * Advances a replay without queueing its entire history in Rust. Only one recorded
 * change waits at a time, regardless of the run length or playback batch size.
 * `nextChange` belongs to the replay plan and advances as changes are scheduled.
 */
export function advanceReplay(explorer, plan, samples) {
  const end = explorer.completed() + samples;
  while (plan.nextChange < plan.changes.length && plan.changes[plan.nextChange] <= end) {
    const sample = plan.changes[plan.nextChange];
    explorer.scheduleChange(sample);
    plan.nextChange += 1;
    explorer.run(sample - explorer.completed());
    if (explorer.hasFailed()) return;
  }
  const remaining = end - explorer.completed();
  if (remaining > 0) explorer.run(remaining);
}
