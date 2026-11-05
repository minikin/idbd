import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { test } from "node:test";

test("malformed URLs return 400 without stopping the server", { timeout: 10000 }, async (t) => {
  const child = spawn(process.execPath, ["web/serve.mjs", "0"], {
    cwd: new URL("../../", import.meta.url),
    env: { ...process.env, HOST: "127.0.0.1", PORT: "0" },
    stdio: ["ignore", "pipe", "pipe"],
  });
  t.after(async () => {
    if (child.exitCode === null && child.signalCode === null) {
      const exited = once(child, "exit");
      child.kill();
      await exited;
    }
  });
  let output = "";
  const base = await new Promise((resolve, reject) => {
    child.once("error", reject);
    child.once("exit", (code) => reject(new Error(`server exited with ${code}`)));
    child.stdout.on("data", (chunk) => {
      output += chunk;
      const match = output.match(/http:\/\/127\.0\.0\.1:\d+/);
      if (match) resolve(match[0]);
    });
  });
  for (const path of ["/%", "/%E0%A4%A"]) {
    assert.equal((await fetch(base + path)).status, 400, path);
    assert.equal((await fetch(base + "/")).status, 200, "server survives invalid input");
  }
});
