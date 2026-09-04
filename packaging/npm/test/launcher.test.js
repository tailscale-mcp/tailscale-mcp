// The launcher, checked without a release to download from.
//
// The one thing this package exists to do is refuse to run a binary that does
// not hash to what the release says it does, so that is the test that matters:
// a real archive, a real `SHA256SUMS`, a real unpack, and then the same thing
// again with one byte changed.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";
import { fileURLToPath } from "node:url";

import { archiveName, binaryName, cacheDir, digestIn, ensureBinary, sha256, targetFor } from "../lib/launcher.js";

const VERSION = "9.9.9";
const scratch = mkdtempSync(join(tmpdir(), "launcher-test-"));
after(() => rmSync(scratch, { recursive: true, force: true }));

/** A release archive of the shape the release job builds, as bytes. */
function makeArchive(version, target, platform = process.platform) {
  const staging = join(scratch, `build-${target}`);
  const inner = `tailscale-mcp-${version}-${target}`;
  rmSync(staging, { recursive: true, force: true });
  mkdirSync(join(staging, inner), { recursive: true });
  writeFileSync(join(staging, inner, binaryName(platform)), "#!/bin/sh\necho ran\n");
  writeFileSync(join(staging, inner, "LICENSE"), "Apache-2.0\n");
  const name = archiveName(version, target);
  const tar = spawnSync("tar", ["czf", join(staging, name), "-C", staging, inner]);
  assert.equal(tar.status, 0, "the test needs tar to build an archive");
  return readFileSync(join(staging, name));
}

/** Answers a release would give, as a fetch the launcher can be handed. */
function releaseServing(files) {
  return async (url) => {
    const name = url.split("/").pop();
    if (!(name in files)) throw new Error(`${url} answered 404 Not Found`);
    return files[name];
  };
}

test("a machine with no binary is told so, and told what there is", () => {
  assert.equal(targetFor("darwin", "arm64"), "aarch64-apple-darwin");
  assert.equal(targetFor("linux", "x64"), "x86_64-unknown-linux-gnu");
  assert.equal(targetFor("win32", "x64"), "x86_64-pc-windows-msvc");
  assert.throws(() => targetFor("freebsd", "x64"), /no tailscale-mcp binary for freebsd\/x64/);
  // Naming the alternatives matters: the answer to "there is no binary" is
  // usually "you are on the wrong architecture", which the list makes obvious.
  assert.throws(() => targetFor("linux", "riscv64"), /darwin\/arm64, darwin\/x64/);
});

test("the cache is somewhere the platform keeps caches", () => {
  assert.equal(cacheDir("linux", {}, "/home/x"), "/home/x/.cache/tailscale-mcp");
  assert.equal(cacheDir("linux", { XDG_CACHE_HOME: "/c" }, "/home/x"), "/c/tailscale-mcp");
  assert.equal(cacheDir("win32", { LOCALAPPDATA: "C:\\c" }, "C:\\u"), join("C:\\c", "tailscale-mcp"));
});

test("SHA256SUMS is read the way sha256sum writes it", () => {
  const sums = [
    "0000000000000000000000000000000000000000000000000000000000000000  other.tar.gz",
    "1111111111111111111111111111111111111111111111111111111111111111  wanted.tar.gz",
    "",
  ].join("\n");
  assert.equal(digestIn(sums, "wanted.tar.gz"), "1".repeat(64));
  assert.equal(digestIn(sums, "absent.tar.gz"), null);
  // `sha256sum -b` marks binary mode with a star, and a release built that way
  // is still a release this has to read.
  assert.equal(digestIn(`${"2".repeat(64)} *binary.tar.gz\n`, "binary.tar.gz"), "2".repeat(64));
});

test("a matching archive is unpacked, cached, and used again", async () => {
  const target = targetFor(process.platform, process.arch);
  const archive = makeArchive(VERSION, target);
  const name = archiveName(VERSION, target);
  const cache = join(scratch, "cache-good");
  const files = {
    [name]: archive,
    SHA256SUMS: Buffer.from(`${sha256(archive)}  ${name}\n`),
  };

  let fetches = 0;
  const counting = (url) => {
    fetches += 1;
    return releaseServing(files)(url);
  };
  const where = {
    version: VERSION,
    platform: process.platform,
    arch: process.arch,
    cache,
    fetchImpl: counting,
  };

  const binary = await ensureBinary(where);
  assert.ok(existsSync(binary), "the binary should be where it says it is");
  assert.equal(fetches, 2, "the sums and the archive, once each");

  // Second time round it is already there, so nothing is fetched again.
  assert.equal(await ensureBinary(where), binary);
  assert.equal(fetches, 2);

  // And nothing is left behind: a scratch directory that survived would be a
  // half-unpacked binary the next run could find.
  assert.deepEqual(
    readdirSync(cache).filter((entry) => entry.startsWith("download-")),
    []
  );
});

test("an archive that does not match the release is refused before it is unpacked", async () => {
  const target = targetFor(process.platform, process.arch);
  const archive = makeArchive(VERSION, target);
  const name = archiveName(VERSION, target);
  const cache = join(scratch, "cache-tampered");
  const files = {
    [name]: archive,
    // The archive somebody swapped in does not hash to what the release said.
    SHA256SUMS: Buffer.from(`${"0".repeat(64)}  ${name}\n`),
  };

  await assert.rejects(
    ensureBinary({
      version: VERSION,
      platform: process.platform,
      arch: process.arch,
      cache,
      fetchImpl: releaseServing(files),
    }),
    /refusing to run it/
  );
  assert.ok(!existsSync(join(cache, VERSION)), "nothing should have been unpacked");
});

test("an archive the release does not list is refused too", async () => {
  const target = targetFor(process.platform, process.arch);
  const archive = makeArchive(VERSION, target);
  const name = archiveName(VERSION, target);
  const cache = join(scratch, "cache-unlisted");

  await assert.rejects(
    ensureBinary({
      version: VERSION,
      platform: process.platform,
      arch: process.arch,
      cache,
      fetchImpl: releaseServing({ [name]: archive, SHA256SUMS: Buffer.from("\n") }),
    }),
    /nothing can vouch for it/
  );
  assert.ok(!existsSync(join(cache, VERSION)), "nothing should have been unpacked");
});

test("the launcher runs what it has already verified, and gets out of the way", (t) => {
  // Everything above tests `lib/`; this tests `bin/`, which is what `npx`
  // actually runs. It cannot be handed a fetch, so the cache is warmed
  // first — the same state the second `npx` on a machine finds — and what is
  // left to check is the part that has nothing to do with downloading:
  // arguments reach the server, its streams are its own, and its exit status
  // comes back out.
  if (process.platform === "win32") {
    t.skip("the stand-in binary is a shell script");
    return;
  }
  // The version is the package's own, because that is the one `bin/` asks for.
  const { version } = JSON.parse(readFileSync(new URL("../package.json", import.meta.url), "utf8"));
  const home = join(scratch, "warm");
  const cache = cacheDir(process.platform, { XDG_CACHE_HOME: join(home, "cache") }, home);
  const target = targetFor(process.platform, process.arch);
  mkdirSync(join(cache, version, target), { recursive: true });
  writeFileSync(join(cache, version, target, binaryName(process.platform)), '#!/bin/sh\necho "ran: $*"\nexit 7\n', {
    mode: 0o755,
  });

  const ran = spawnSync(process.execPath, [fileURLToPath(new URL("../bin/tailscale-mcp.js", import.meta.url)), "--preset", "full"], {
    env: { ...process.env, XDG_CACHE_HOME: join(home, "cache") },
    encoding: "utf8",
  });

  assert.equal(ran.stdout, "ran: --preset full\n", "the arguments should have reached the server");
  assert.equal(ran.stderr, "", "the launcher should have said nothing");
  assert.equal(ran.status, 7, "the server's exit status should have come back out");
});
