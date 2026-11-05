# Project review: idbd

## Resolution

All four confirmed findings below have been fixed. The original review is retained
as the record of the pre-fix behavior.

- **Replay:** [incremental playback](worktree://38eb5084-c743-44b0-a010-43814cacfe04/web/src/replay.js#L1)
  queues at most one recorded change in Rust. Node tests cover 10,001 changes with
  three batch sizes and failure paths; a native session test compares learner state,
  history, and totals after 10,001 changes. A Playwright regression also exercises
  loading, exporting, and the replay button at two speeds.
- **Server:** [URL validation](worktree://38eb5084-c743-44b0-a010-43814cacfe04/web/serve.mjs#L38)
  responds with 400 for malformed escapes. An HTTP regression verifies both malformed
  URLs and subsequent successful requests to the same server.
- **Rustdoc:** [function links](worktree://38eb5084-c743-44b0-a010-43814cacfe04/crates/idbd-lab/src/report.rs#L28)
  explicitly name `write()`. The strict CI-equivalent documentation command now passes.
- **Statistics:** [overflow fallbacks](worktree://38eb5084-c743-44b0-a010-43814cacfe04/crates/idbd-lab/src/stats.rs#L52)
  scale the mean and, when necessary, opposite-sign deviations. Ordinary finite
  calculations retain their previous arithmetic; regression tests include identical
  large values, zeros, and mixed signs near `f64::MAX`.

Post-fix checks passed: `cargo test --workspace --all-features` (247 tests and 11
doctests), `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
`cargo fmt --all -- --check`, `RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps`,
`node --test web/node-tests/*.test.js` (7 tests), JavaScript syntax checks for the
modified Explorer and browser test, and `git diff --check`. The Node tests are now
included in `npm test`, `just web-test`, and the CI web job.

The WASM build and Playwright regressions remain unrun locally: the WASM target and
web dependencies are not installed. The Node replay tests use a bounded-queue test
double; the native session test uses the actual Rust learners.

## Summary and scope

The published `idbd` library has a strong design: validated configuration, atomic
updates, allocation-free learning, and unusually good numerical regression tests.
The most important issue found is outside the core: an Explorer run can record and
export a valid history that its own replay path cannot load.

This review covers the core IDBD and Autostep implementations, configuration and
serialization, selected lab measurement/session code, the WASM boundary, Explorer
playback/export/import, the story game, the development server, and CI configuration.
The public core was assessed as a library; the unpublished lab, adapter, and browser
code were assessed as applications. The workspace uses edition 2024 and declares
Rust 1.85 as its minimum version. This is a targeted project review, not an exhaustive
audit of every file or an independent verification of the published experiment results.

No implementation files were changed during the original review.

## Must fix

### 1. [P2] Valid long runs cannot replay their own change history

**Locations:** [Explorer build, `web/src/experiment.js:205–211`](worktree://38eb5084-c743-44b0-a010-43814cacfe04/web/src/experiment.js#L205-L211),
[history recording, `crates/idbd-lab/src/session.rs:433–434`](worktree://38eb5084-c743-44b0-a010-43814cacfe04/crates/idbd-lab/src/session.rs#L433-L434),
and [schedule limit, `session.rs:469–477`](worktree://38eb5084-c743-44b0-a010-43814cacfe04/crates/idbd-lab/src/session.rs#L469-L477).

**Rule:** Preserve observable behavior across round trips; test actual public
workflows and boundary cases (Rust best-practices skill, “Tests”).

**Problem:** The session keeps every applied user-requested change, but allows only
10,000 changes to wait in the schedule. Explorer replay installs the entire recorded
history before processing any samples. After 10,001 applied changes, both loading the
export and “Replay this run” fail while scheduling change 10,001. The latter click
handler does not catch the error. The schedule limit is reasonable for untrusted
input, but it currently also limits the length of a legitimate completed history.

**Evidence:** A temporary harness linked against the actual lab library completed
10,001 samples, requesting a change before each one. It recorded 10,001 changes.
Scheduling that history into a fresh session failed at sample 10,001 with
`at most 10000 changes can wait`.

**Fix:** Keep the bounded Rust schedule, but feed replay changes into it incrementally.
Retain an index into the validated replay history and refill the pending schedule
after batches consume entries. The following is the essential queueing rule, not a
complete patch; expose or share the capacity rather than duplicating its value:

```js
while (nextChange < replay.changes.length && pending < scheduleCapacity) {
  explorer.scheduleChange(replay.changes[nextChange]);
  nextChange += 1;
  pending += 1;
}
```

**Regression test:** Export and replay a run with `SCHEDULE_CAPACITY + 1` applied
changes, under more than one batch size. Compare final weights and metrics. Also
exercise the replay button and ensure replay construction errors remain visible
rather than becoming unhandled exceptions.

### 2. [P2] A malformed URL terminates the development server

**Location:** [Request handling, `web/serve.mjs:28–44`](worktree://38eb5084-c743-44b0-a010-43814cacfe04/web/serve.mjs#L28-L44).

**Rule:** Expected invalid input must follow an error path rather than terminate
the application (Rust best-practices skill, “Errors and panics”; the same principle
applies at this JavaScript input boundary).

**Problem:** `decodeURIComponent` throws on malformed percent escapes, but
`resolvePath(request.url)` runs before the handler's `try`. Because the callback is
async and its rejection is not handled, Node exits. One malformed request interrupts
the demo and any browser tests using that server. The default loopback binding
limits exposure; this is not a finding about the deployed static GitHub Pages site.

**Evidence:** Started a temporary server with Node 24.22.0. `GET /` returned 200;
`GET /%` reset the connection and the server exited with code 1 and
`URIError: URI malformed` at line 29.

**Fix:** Catch URL parsing/decoding failures separately and respond with 400:

```js
let target;
try {
  target = resolvePath(request.url ?? "/");
} catch {
  response.writeHead(400, { "content-type": "text/plain; charset=utf-8" })
    .end("bad request");
  return;
}
```

Keep the existing forbidden-path and filesystem handling after this check.

**Regression test:** Request `/%` and `/%E0%A4%A`, expect 400, then request `/` and
verify the same server is still running.

### 3. [P2] Ambiguous documentation links fail the configured CI docs gate

**Locations:** [`crates/idbd-lab/src/report.rs:28,32`](worktree://38eb5084-c743-44b0-a010-43814cacfe04/crates/idbd-lab/src/report.rs#L28-L32)
and [CI rustdoc flags, `.github/workflows/ci.yml:17–20`](worktree://38eb5084-c743-44b0-a010-43814cacfe04/.github/workflows/ci.yml#L17-L20).

**Rule:** Public documentation must resolve its intra-doc links
(`rustdoc::broken_intra_doc_links`).

**Problem:** Both links written as `` [`write`] `` are ambiguous between the local
function and the standard macro. An ordinary documentation build emits warnings,
but CI sets `RUSTDOCFLAGS=-D warnings`, turning them into build failures.

**Evidence:** `RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps` failed with
both ambiguous-link errors on Rust 1.97.1.

**Fix:** Use the function disambiguator in both comments:

```rust
/// Writes every `(name, contents)` pair into `dir` with [`write()`], in order.
// ...
/// Returns the first failure from [`write()`]. Files before it stay written.
```

**Regression check:** Rerun the exact strict rustdoc command above.

## Should fix

### 4. [P2] Summary statistics overflow even when the answer is representable

**Location:** [Mean calculation, `crates/idbd-lab/src/stats.rs:22–27`](worktree://38eb5084-c743-44b0-a010-43814cacfe04/crates/idbd-lab/src/stats.rs#L22-L27).

**Rule:** Avoid invalid intermediate arithmetic when valid inputs have a
representable result; cover numerical boundary behavior with property/regression
tests (Rust best-practices skill, “Tests”).

**Problem:** The mean is calculated by summing the original values before dividing.
`Summary::new(&[1e308, 1e308])` returns an infinite mean and infinite standard error,
although the correct values are `1e308` and zero. The standard-error implementation
already scales deviations to avoid overflow, but that protection is ineffective if
the mean has overflowed first. This affects extreme measurements; the review did not
establish any impact on the checked-in experiment tables.

**Evidence:** A temporary harness calling the actual compiled `Summary` API printed
`mean=inf, SE=inf` for those two identical finite inputs.

**Fix:** Scale finite observations before summation and rescale the mean. Preserve
or explicitly define the existing behavior for non-finite inputs. The finite-input
branch can use:

```rust
let scale = values.iter().map(|value| value.abs()).fold(0.0, f64::max);
let mean = if scale == 0.0 {
    0.0
} else {
    (values.iter().map(|value| value / scale).sum::<f64>() / n) * scale
};
```

**Regression tests:** Identical `1e308` observations must produce a finite mean and
zero standard error; include large mixed-sign observations and all-zero input.

## Consider

- **Enable automatic gates when the project is ready.** The automatic triggers in
  [`.github/workflows/ci.yml:3–8`](worktree://38eb5084-c743-44b0-a010-43814cacfe04/.github/workflows/ci.yml#L3-L8)
  are deliberately commented out. This is not treated as a bug, but currently PRs
  do not automatically exercise the substantial checks defined below them.
- **Do not add lint churn merely to satisfy the review command.** Explicit
  `-W clippy::pedantic` overrides intentional workspace allowances and produced many
  low-value suggestions, chiefly `must_use` and panic documentation in unpublished
  application code. The project's own warnings-as-errors Clippy command passes.

## What's already good

- [Candidate-buffer commits](worktree://38eb5084-c743-44b0-a010-43814cacfe04/crates/idbd/src/learner.rs#L211-L236)
  make failed core updates atomic without per-example allocation.
- [Serialization reconstructs validated configuration and state](worktree://38eb5084-c743-44b0-a010-43814cacfe04/crates/idbd/src/serialization.rs#L105-L112),
  rather than trusting deserialized private fields.
- [Session scratch learners](worktree://38eb5084-c743-44b0-a010-43814cacfe04/crates/idbd-lab/src/session.rs#L332-L354)
  keep the comparison at the last mutually completed sample when a learner fails.
- The test suite includes independent numerical fixtures, failure atomicity,
  bitwise checkpoint continuation, allocation checks, and property tests—not just
  happy-path examples. The README also states the experimental limitations rather
  than presenting synthetic results as a universal advantage.

## Validation performed

Environment: Rust 1.97.1, Node.js 24.22.0, macOS.

| Check | Result |
| --- | --- |
| `cargo fmt --check` | Passed. |
| `cargo clippy --all-targets --all-features -- -W clippy::pedantic` | Completed successfully with warnings; not a clean pedantic run. |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | Passed. |
| `cargo test --workspace --all-features` | Passed: 245 unit/integration tests and 11 doctests. |
| `cargo doc --workspace --no-deps` | Completed with two ambiguous-link warnings. |
| `RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps` | Failed; finding 3. |
| Temporary Rust harness linked to compiled workspace libraries | Reproduced findings 1 and 4. |
| Temporary HTTP server and malformed URL request | Reproduced finding 2. |

**Not run:** WASM build and Playwright/browser tests (`wasm32-unknown-unknown` is not
installed), minimum-Rust-version validation, other operating systems, release-mode
allocation checks, full experiment regeneration, coverage/CRAP gates, mutation
testing, fuzzing, dependency advisory checks, and package/publication validation.
Browser-specific behavior beyond the inspected code remains unverified.
