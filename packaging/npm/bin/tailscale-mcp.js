#!/usr/bin/env node
// The launcher.
//
// `npx @tailscale-mcp/tailscale-mcp` is one line in an MCP client's
// configuration, and this is what it runs: fetch the release binary for this
// machine the first time, check it against the release's own `SHA256SUMS`, and
// then get out of the way — arguments, standard streams, exit status and
// signals all belong to the server, not to this.

import { spawn } from "node:child_process";
import { readFileSync } from "node:fs";
import { constants } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { ensureBinary, machine } from "../lib/launcher.js";

const here = dirname(fileURLToPath(import.meta.url));
// The launcher's version is the server's version: they are released together,
// so the package that asks for 1.2.3 fetches the 1.2.3 binary.
const { version } = JSON.parse(readFileSync(join(here, "..", "package.json"), "utf8"));

let binary;
try {
  binary = await ensureBinary({ version, ...machine() });
} catch (error) {
  process.stderr.write(`tailscale-mcp: ${error.message}\n`);
  process.exit(1);
}

// `stdio: "inherit"` because this server speaks MCP over the standard streams
// when it is run this way: anything this process put on them would be a
// protocol error in the client.
const server = spawn(binary, process.argv.slice(2), { stdio: "inherit" });
server.on("error", (error) => {
  process.stderr.write(`tailscale-mcp: could not run ${binary}: ${error.message}\n`);
  process.exit(1);
});
// Pass on what a client sends us, so stopping the launcher stops the server.
for (const signal of ["SIGINT", "SIGTERM", "SIGHUP"]) {
  process.on(signal, () => server.kill(signal));
}
server.on("exit", (code, signal) => {
  // A process killed by a signal has no exit code; the shell convention is
  // 128 plus the signal's number, and that is what a caller expects to see.
  if (signal) process.exit(128 + (constants.signals[signal] ?? 15));
  process.exit(code ?? 1);
});
