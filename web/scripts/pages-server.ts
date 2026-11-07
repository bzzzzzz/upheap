// Serves the local build from dist as GitHub Pages does, for the Playwright tests against it
// (playwright.local.config.ts): files under the base path, and 404.html with status 404 for a path without a file,
// so a deep link starts the app the way it will on Pages. No rewrites and no special headers, which Pages can't send
// either.
//
//   UPHEAP_BASE=/upheap/ PORT=8140 node scripts/pages-server.ts
import { readFile } from "node:fs/promises";
import { createServer } from "node:http";
import { extname, join, normalize } from "node:path";

const root = new URL("../dist/", import.meta.url).pathname;
const base = process.env.UPHEAP_BASE ?? "/";
const port = Number(process.env.PORT ?? 8140);

const TYPES: Record<string, string> = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript",
  ".mjs": "text/javascript",
  ".css": "text/css",
  ".wasm": "application/wasm",
  ".json": "application/json",
  ".svg": "image/svg+xml",
};

/** The file for `pathname` under `root`, or null when there is none to serve. */
async function readPath(pathname: string): Promise<{ body: Buffer; type: string } | null> {
  if (!pathname.startsWith(base)) {
    return null;
  }
  const relative = normalize(decodeURIComponent(pathname.slice(base.length)));
  if (relative.startsWith("..")) {
    return null;
  }
  const path = join(root, relative === "." || relative.endsWith("/") ? join(relative, "index.html") : relative);
  try {
    return { body: await readFile(path), type: TYPES[extname(path)] ?? "application/octet-stream" };
  } catch {
    return null;
  }
}

createServer((request, response) => {
  void (async () => {
    const { pathname } = new URL(request.url ?? "/", "http://localhost");
    const found = await readPath(pathname);
    if (found) {
      response.writeHead(200, { "content-type": found.type });
      response.end(found.body);
      return;
    }
    response.writeHead(404, { "content-type": TYPES[".html"] });
    response.end(await readFile(join(root, "404.html")));
  })();
}).listen(port, "127.0.0.1", () => console.log(`serving dist at http://127.0.0.1:${port}${base}`));
