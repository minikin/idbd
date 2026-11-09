#!/usr/bin/env node
// A static file server for this directory. Used by `just web-serve` and by the
// Playwright config, which needs `application/wasm` on the module.

import { createReadStream } from "node:fs";
import { stat } from "node:fs/promises";
import { createServer } from "node:http";
import { dirname, extname, join, normalize, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = dirname(fileURLToPath(import.meta.url));
const PORT = Number(process.env.PORT ?? process.argv[2] ?? 8173);
const HOST = process.env.HOST ?? "127.0.0.1";

const TYPES = new Map([
  [".html", "text/html; charset=utf-8"],
  [".css", "text/css; charset=utf-8"],
  [".js", "text/javascript; charset=utf-8"],
  [".mjs", "text/javascript; charset=utf-8"],
  [".json", "application/json; charset=utf-8"],
  [".wasm", "application/wasm"],
  [".svg", "image/svg+xml"],
  [".ico", "image/x-icon"],
  [".png", "image/png"],
  [".jpg", "image/jpeg"],
  [".txt", "text/plain; charset=utf-8"],
  [".xml", "application/xml; charset=utf-8"],
  [".webmanifest", "application/manifest+json"],
  [".woff2", "font/woff2"],
]);

function resolvePath(url) {
  const pathname = decodeURIComponent(new URL(url, "http://localhost").pathname);
  const relative = normalize(pathname).replace(/^(\.\.(\/|\\|$))+/, "");
  const target = resolve(join(ROOT, relative === sep ? "index.html" : relative));
  if (target !== ROOT && !target.startsWith(ROOT + sep)) {
    return null;
  }
  return target;
}

const server = createServer(async (request, response) => {
  let target;
  try {
    target = resolvePath(request.url ?? "/");
  } catch {
    response.writeHead(400, { "content-type": "text/plain; charset=utf-8" }).end("bad request");
    return;
  }
  if (!target) {
    response.writeHead(403).end("forbidden");
    return;
  }
  try {
    // Cloudflare serves /explorer from explorer.html, and the pages link to the clean address
    const info = await stat(target).catch(() => (extname(target) ? null : stat((target += ".html"))));
    if (!info) throw new Error("not found");
    const file = info.isDirectory() ? join(target, "index.html") : target;
    const size = info.isDirectory() ? (await stat(file)).size : info.size;
    response.writeHead(200, {
      "content-type": TYPES.get(extname(file)) ?? "application/octet-stream",
      "content-length": size,
      "cache-control": "no-store",
    });
    createReadStream(file).pipe(response);
  } catch {
    response.writeHead(404, { "content-type": "text/plain; charset=utf-8" }).end("not found");
  }
});

server.listen(PORT, HOST, () => {
  process.stdout.write(`IDBD Explorer on http://${HOST}:${server.address().port}\n`);
});
