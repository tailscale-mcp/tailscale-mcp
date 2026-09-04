// The pieces the launcher is made of, kept out of `bin/` so they can be tested
// without running the server.
//
// The shape of the thing: an npm package that carries no binary of its own,
// works out which release archive this machine wants, downloads it and the
// release's `SHA256SUMS`, refuses to go on unless the archive hashes to what
// that file says, and only then unpacks and runs it. The checksum is the whole
// point of the package existing rather than telling people to `curl | tar`.

import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { chmodSync, mkdirSync, mkdtempSync, renameSync, rmSync, statSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

/** The GitHub release the binaries come from. */
const RELEASES = "https://github.com/tailscale-mcp/tailscale-mcp/releases/download";

/** The file every release carries, listing each archive and its hash. */
const SUMS = "SHA256SUMS";

/**
 * Node's platform and architecture names, mapped to the Rust target triples
 * the release archives are named for. Anything not here has no binary, which
 * is a thing to say plainly rather than to fail at while unpacking.
 */
const TARGETS = {
  "darwin/arm64": "aarch64-apple-darwin",
  "darwin/x64": "x86_64-apple-darwin",
  "linux/arm64": "aarch64-unknown-linux-gnu",
  "linux/x64": "x86_64-unknown-linux-gnu",
  "win32/x64": "x86_64-pc-windows-msvc",
};

/** The target triple for a machine, or an error naming what it is. */
export function targetFor(platform, arch) {
  const target = TARGETS[`${platform}/${arch}`];
  if (!target) {
    const known = Object.keys(TARGETS).sort().join(", ");
    throw new Error(`no tailscale-mcp binary for ${platform}/${arch}; there are binaries for ${known}`);
  }
  return target;
}

/** What the archive for one version and target is called in the release. */
export function archiveName(version, target) {
  return `tailscale-mcp-${version}-${target}.tar.gz`;
}

/** The binary's name inside the archive, which carries `.exe` on Windows. */
export function binaryName(platform) {
  return platform === "win32" ? "tailscale-mcp.exe" : "tailscale-mcp";
}

/** Where a verified binary is kept, so the download happens once. */
export function cacheDir(platform, env, home) {
  const base =
    platform === "win32"
      ? env.LOCALAPPDATA || join(home, "AppData", "Local")
      : env.XDG_CACHE_HOME || join(home, ".cache");
  return join(base, "tailscale-mcp");
}

/** The hash of some bytes, written the way `sha256sum` writes it. */
export function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

/**
 * The hash `sums` gives for `name`.
 *
 * `SHA256SUMS` is `<hex>  <name>` a line at a time, which is what
 * `sha256sum -c` reads and what the release job writes.
 */
export function digestIn(sums, name) {
  for (const line of sums.split("\n")) {
    const match = line.match(/^([0-9a-f]{64})\s+\*?(.+?)\s*$/);
    if (match && match[2] === name) return match[1];
  }
  return null;
}

/** Fetch a URL, or say which one did not answer. */
async function fetchBytes(url) {
  const answer = await fetch(url, { redirect: "follow" });
  if (!answer.ok) {
    throw new Error(`${url} answered ${answer.status} ${answer.statusText}`);
  }
  return Buffer.from(await answer.arrayBuffer());
}

/**
 * The path to a verified binary for this machine, downloading it if it is not
 * already here.
 *
 * Nothing is unpacked before the archive has been hashed and the hash has been
 * found in the release's own `SHA256SUMS`: an archive that does not match is a
 * download that goes in the bin, not one to run and find out about.
 */
export async function ensureBinary({ version, platform, arch, cache, fetchImpl = fetchBytes }) {
  const target = targetFor(platform, arch);
  const binary = binaryName(platform);
  const kept = join(cache, version, target, binary);
  try {
    if (statSync(kept).isFile()) return kept;
  } catch {
    // Not there yet, which is the usual case exactly once.
  }

  const name = archiveName(version, target);
  const where = `${RELEASES}/v${version}`;
  const [sums, archive] = await Promise.all([fetchImpl(`${where}/${SUMS}`), fetchImpl(`${where}/${name}`)]);

  const wanted = digestIn(sums.toString("utf8"), name);
  if (!wanted) {
    throw new Error(`${where}/${SUMS} does not list ${name}, so nothing can vouch for it`);
  }
  const got = sha256(archive);
  if (got !== wanted) {
    throw new Error(`${name} hashes to ${got} and the release says ${wanted}; refusing to run it`);
  }

  // Unpack somewhere of its own and move the binary into place, so a run that
  // dies half-way through leaves no half-unpacked binary for the next one to
  // find and trust. The scratch directory is inside the cache rather than in
  // the system temporary directory, because the move at the end has to be a
  // rename and a rename cannot cross a filesystem.
  mkdirSync(cache, { recursive: true });
  const scratch = mkdtempSync(join(cache, "download-"));
  try {
    const downloaded = join(scratch, name);
    writeFileSync(downloaded, archive);
    const untar = spawnSync("tar", ["-xzf", downloaded, "-C", scratch], { stdio: "inherit" });
    if (untar.status !== 0) {
      throw new Error(`tar could not unpack ${name}`);
    }
    mkdirSync(join(cache, version, target), { recursive: true });
    const unpacked = join(scratch, `tailscale-mcp-${version}-${target}`, binary);
    chmodSync(unpacked, 0o755);
    // Two `npx` runs at once would otherwise race here, and Windows refuses a
    // rename onto a file that exists.
    rmSync(kept, { force: true });
    renameSync(unpacked, kept);
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
  return kept;
}

/** Everything the launcher needs about the machine it is on. */
export function machine(env = process.env) {
  return {
    platform: process.platform,
    arch: process.arch,
    cache: cacheDir(process.platform, env, homedir()),
  };
}
