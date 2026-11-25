import { mkdir, open, readFile, unlink, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { Context, Effect } from "effect";
import { required } from "./config";

interface BlobStore {
  put: (key: string, bytes: Uint8Array) => Promise<void>;
  get: (key: string) => Promise<Uint8Array>;
  range: (key: string, offset: number, length: number) => Promise<Uint8Array>;
  delete: (key: string) => Promise<void>;
}

export class Storage extends Context.Service<Storage, BlobStore>()("subspace/Storage") {}

export function storage(env: NodeJS.ProcessEnv): BlobStore {
  if (env.STORAGE_DRIVER === "s3") {
    const client = new Bun.S3Client({
      bucket: required(env, "S3_BUCKET"),
      endpoint: env.S3_ENDPOINT || undefined,
      region: env.S3_REGION || undefined,
      accessKeyId: env.S3_ACCESS_KEY_ID || undefined,
      secretAccessKey: env.S3_SECRET_ACCESS_KEY || undefined,
      sessionToken: env.S3_SESSION_TOKEN || undefined,
    });
    return {
      async put(key, bytes) {
        await client.write(key, bytes);
      },
      async get(key) {
        return client.file(key).bytes();
      },
      async range(key, offset, length) {
        return client
          .file(key)
          .slice(offset, offset + length)
          .bytes();
      },
      async delete(key) {
        await client.delete(key);
      },
    };
  }
  if (env.STORAGE_DRIVER && env.STORAGE_DRIVER !== "local") {
    throw new Error("STORAGE_DRIVER must be local or s3");
  }
  const directory = resolve(env.STORAGE_PATH || ".data/blobs");
  return {
    async put(key, bytes) {
      await mkdir(directory, { recursive: true, mode: 0o700 });
      await writeFile(join(directory, key), bytes, { mode: 0o600, flag: "wx" });
    },
    async get(key) {
      return readFile(join(directory, key));
    },
    async range(key, offset, length) {
      const file = await open(join(directory, key), "r");
      try {
        const buffer = Buffer.alloc(length);
        let count = 0;
        while (count < length) {
          const { bytesRead } = await file.read(buffer, count, length - count, offset + count);
          if (!bytesRead) break;
          count += bytesRead;
        }
        return buffer.subarray(0, count);
      } finally {
        await file.close();
      }
    },
    async delete(key) {
      try {
        await unlink(join(directory, key));
      } catch (error) {
        if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error;
      }
    },
  };
}

export const storageOperation = <A>(run: () => Promise<A>) =>
  Effect.tryPromise({ try: run, catch: () => new Error("Object storage operation failed") });
