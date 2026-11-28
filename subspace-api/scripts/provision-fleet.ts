import { constants } from "node:fs";
import { open } from "node:fs/promises";
import { Effect, Schema } from "effect";
import { databaseLayer } from "../src/db";
import { BootstrapRefused } from "../src/pairing-owner";
import { isProblem } from "../src/problems";
import { FleetProvisionSpec, provisionFleet, provisioningConfiguration } from "../src/provisioning";

const usage = "Usage: bun run --silent auth:provision-fleet [--file PATH]";
const maxInput = 64 * 1024;

function fail(message: string): never {
  console.error(message);
  process.exit(1);
}

async function readInput(path?: string) {
  const file = path
    ? await open(path, constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK)
    : undefined;
  try {
    if (file) {
      const stat = await file.stat();
      if (!stat.isFile() || stat.uid !== 0 || (stat.mode & 0o077) !== 0)
        throw new Error("Not a private root-owned file");
    }
    const decoder = new TextDecoder("utf-8", { fatal: true });
    let bytes = 0;
    let input = "";
    for await (const chunk of file
      ? file.createReadStream({ autoClose: false })
      : Bun.stdin.stream()) {
      bytes += chunk.byteLength;
      if (bytes > maxInput) throw new Error("Input exceeds limit");
      input += decoder.decode(chunk, { stream: true });
    }
    return JSON.parse(input + decoder.decode()) as unknown;
  } finally {
    await file?.close();
  }
}

if (process.getuid?.() !== 0) fail("Run this command as root on the API host.");

const [flag, path, ...extra] = process.argv.slice(2);
if (flag !== undefined && (flag !== "--file" || !path || extra.length > 0)) fail(usage);

const spec = await readInput(path)
  .then((input) =>
    Schema.decodeUnknownSync(FleetProvisionSpec, { onExcessProperty: "error" })(input),
  )
  .catch(() =>
    fail(
      "Supply a valid fleet specification (UTF-8 JSON, at most 64 KiB). Use standard input or a private, root-owned regular file.",
    ),
  );

// Checked before the database is opened. The messages name settings, never their values.
const configuration = (() => {
  try {
    return provisioningConfiguration(process.env);
  } catch (error) {
    return fail(error instanceof Error ? error.message : "The API configuration is not usable.");
  }
})();

try {
  const result = await Effect.runPromise(
    provisionFleet(spec, configuration.reserved).pipe(
      Effect.provide(databaseLayer(configuration.url)),
    ),
  );
  // This is the only place the plaintext grant leaves the process.
  process.stdout.write(`${JSON.stringify(result)}\n`);
} catch (error) {
  if (error instanceof BootstrapRefused) fail(error.message);
  if (
    isProblem(error) &&
    error.status < 500 &&
    "detail" in error &&
    typeof error.detail === "string"
  )
    fail(error.detail);
  fail(
    "The fleet could not be provisioned. Check the fleet specification, the database and its migrations.",
  );
}
