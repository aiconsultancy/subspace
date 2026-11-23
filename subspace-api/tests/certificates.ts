/**
 * Test support: self-signed X.509 certificates built here, in DER, so the tests need no OpenSSL,
 * and agent tokens minted the way a gateway would mint them.
 */
import { createHash, generateKeyPairSync, sign, type KeyObject } from "node:crypto";
import { SignJWT } from "jose";

const length = (size: number) => {
  if (size < 0x80) return Buffer.from([size]);
  const bytes: number[] = [];
  for (let rest = size; rest > 0; rest >>= 8) bytes.unshift(rest & 0xff);
  return Buffer.from([0x80 | bytes.length, ...bytes]);
};
const tlv = (tag: number, ...content: Buffer[]) => {
  const body = Buffer.concat(content);
  return Buffer.concat([Buffer.from([tag]), length(body.length), body]);
};
const sequence = (...parts: Buffer[]) => tlv(0x30, ...parts);
const integer = (value: number) => tlv(0x02, Buffer.from([value]));
const oid = (dotted: string) => {
  const [first, second, ...rest] = dotted.split(".").map(Number);
  const bytes = [first! * 40 + second!];
  for (const part of rest) {
    const chunk: number[] = [part & 0x7f];
    for (let value = part >> 7; value > 0; value >>= 7) chunk.unshift(0x80 | (value & 0x7f));
    bytes.push(...chunk);
  }
  return tlv(0x06, Buffer.from(bytes));
};
const time = (date: Date) =>
  tlv(
    0x17,
    Buffer.from(
      date
        .toISOString()
        .replace(/[-:T]/gu, "")
        .replace(/\.\d{3}Z$/u, "Z")
        .slice(2),
    ),
  );
const name = (common: string) =>
  sequence(tlv(0x31, sequence(oid("2.5.4.3"), tlv(0x0c, Buffer.from(common)))));

export type KeyKind = "ed25519" | "p256" | "p384" | "rsa";

/** A fresh key pair of the kind a certificate will carry. */
export function keyPair(kind: KeyKind) {
  switch (kind) {
    case "ed25519":
      return generateKeyPairSync("ed25519");
    case "p256":
      return generateKeyPairSync("ec", { namedCurve: "P-256" });
    case "p384":
      return generateKeyPairSync("ec", { namedCurve: "P-384" });
    case "rsa":
      return generateKeyPairSync("rsa", { modulusLength: 2048 });
  }
}

const signatureAlgorithm = (kind: KeyKind) =>
  kind === "ed25519"
    ? sequence(oid("1.3.101.112"))
    : kind === "rsa"
      ? sequence(oid("1.2.840.113549.1.1.11"), Buffer.from([0x05, 0x00]))
      : sequence(oid(kind === "p256" ? "1.2.840.10045.4.3.2" : "1.2.840.10045.4.3.3"));

/** A self-signed certificate's DER encoding for a key pair, valid from `notBefore` to `notAfter`. */
export function certificateDer(
  kind: KeyKind,
  keys: { publicKey: KeyObject; privateKey: KeyObject },
  options: { notBefore?: Date; notAfter?: Date; subject?: string } = {},
) {
  const now = Date.now();
  const algorithm = signatureAlgorithm(kind);
  const tbs = sequence(
    tlv(0xa0, integer(2)),
    integer(1),
    algorithm,
    name(options.subject ?? "agent.example.com"),
    sequence(
      time(options.notBefore ?? new Date(now - 60_000)),
      time(options.notAfter ?? new Date(now + 86_400_000)),
    ),
    name(options.subject ?? "agent.example.com"),
    keys.publicKey.export({ type: "spki", format: "der" }),
  );
  const digest = kind === "ed25519" ? null : kind === "p384" ? "sha384" : "sha256";
  const signature = sign(digest, tbs, keys.privateKey);
  return sequence(tbs, algorithm, tlv(0x03, Buffer.from([0]), signature));
}

/** PEM text for DER, in 64-character lines. */
export const pem = (der: Buffer) =>
  `-----BEGIN CERTIFICATE-----\n${der
    .toString("base64")
    .replace(/(.{64})/gu, "$1\n")
    .replace(/\n$/u, "")}\n-----END CERTIFICATE-----\n`;

/** The SHA-256 of the DER encoding in lower-case hex: the fingerprint, as Ergo's certfp. */
export const fingerprintOf = (der: Buffer) => createHash("sha256").update(der).digest("hex");

/** A fresh key and its self-signed certificate, as a gateway generates them for one agent. */
export function agentKey(
  kind: KeyKind = "ed25519",
  options: Parameters<typeof certificateDer>[2] = {},
) {
  const keys = keyPair(kind);
  const der = certificateDer(kind, keys, options);
  return {
    kind,
    ...keys,
    der,
    certificate: pem(der),
    fingerprint: fingerprintOf(der),
    alg:
      kind === "ed25519"
        ? "EdDSA"
        : kind === "p256"
          ? "ES256"
          : kind === "p384"
            ? "ES384"
            : "RS256",
  };
}

export type AgentKey = ReturnType<typeof agentKey>;

/**
 * An agent token as the contract describes it: header `alg`, `typ` and `kid`; claims `iss`,
 * `sub`, `aud`, `iat`, `exp` and a random `jti`. Every part can be replaced to build a refusal.
 */
export function agentToken(
  key: AgentKey,
  audience: string,
  options: {
    header?: Record<string, unknown>;
    claims?: Record<string, unknown>;
    lifetime?: number;
    issuedAt?: number;
    signWith?: KeyObject | Uint8Array;
  } = {},
) {
  const iat = options.issuedAt ?? Math.floor(Date.now() / 1000);
  return new SignJWT({
    iss: "subspace-agent",
    sub: key.fingerprint,
    aud: audience,
    iat,
    exp: iat + (options.lifetime ?? 300),
    jti: crypto.randomUUID(),
    ...options.claims,
  })
    .setProtectedHeader({
      alg: key.alg,
      typ: "subspace-agent+jwt",
      kid: key.fingerprint,
      ...options.header,
    } as never)
    .sign(options.signWith ?? key.privateKey);
}
