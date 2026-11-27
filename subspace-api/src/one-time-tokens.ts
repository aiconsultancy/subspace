import { createHash, randomBytes, timingSafeEqual } from "node:crypto";

/**
 * One-time secrets that name their own row: a prefix such as `senr1.` and the unpadded base64url
 * encoding of 48 bytes, the row's 16-byte ID and 32 random bytes. Only the SHA-256 of the whole
 * text is stored, and it is compared in constant time.
 */
const hashOf = (token: string) => createHash("sha256").update(token, "utf8").digest();

const uuidFromBytes = (bytes: Buffer) => {
  const hex = bytes.toString("hex");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
};

/** A new token with `prefix` for the row `id`, and the hash to store in lower-case hex. */
export function mintOneTimeToken(prefix: string, id: string) {
  const bytes = Buffer.concat([Buffer.from(id.replaceAll("-", ""), "hex"), randomBytes(32)]);
  const token = `${prefix}${bytes.toString("base64url")}`;
  return { token, hash: hashOf(token).toString("hex") };
}

/** The row ID a token names and its hash, if it is `prefix` and 64 base64url characters. */
export function readOneTimeToken(prefix: string, token: string) {
  if (!token.startsWith(prefix)) return undefined;
  const encoded = token.slice(prefix.length);
  if (!/^[A-Za-z0-9_-]{64}$/u.test(encoded)) return undefined;
  const bytes = Buffer.from(encoded, "base64url");
  if (bytes.length !== 48 || bytes.toString("base64url") !== encoded) return undefined;
  return { id: uuidFromBytes(bytes.subarray(0, 16)), hash: hashOf(token) };
}

/** Whether a token's hash is the stored one, compared in constant time. */
export const sameHash = (hash: Buffer, stored: string) =>
  /^[0-9a-f]{64}$/u.test(stored) && timingSafeEqual(hash, Buffer.from(stored, "hex"));
