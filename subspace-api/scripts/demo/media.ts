import type { Client } from "./api";

const assets = new URL("./assets/", import.meta.url);
type Media = { id?: string; mediaId?: string; fileName: string; sha256: string; url: string };

export function image(name: string) {
  return Bun.file(new URL(name, assets));
}

export async function issueFile(
  author: Client,
  issueId: string,
  name: string,
  content: Blob,
  kind: "images" | "attachments",
) {
  const path = `/issues/${issueId}/${kind}`;
  const sha256 = new Bun.CryptoHasher("sha256").update(await content.bytes()).digest("hex");
  const existing = (await author.all<Media>(path)).find(
    (file) => file.fileName === name && file.sha256 === sha256,
  );
  if (existing) return existing;
  const body = new FormData();
  body.set("file", content, name);
  return author.write<Media>("POST", path, body);
}

export async function chatScreenshot(author: Client, name: string) {
  // Stable client ID keeps the IRC transcript unchanged across resets and top-ups.
  const reference = "UPL-dqKj5gLpQpub5p2m1jKAPQ";
  const previous = await author.find<{ reference: string }>(
    `/chat/uploads/${reference}`,
    [404, 410],
  );
  if (previous) return previous.reference;
  const bytes = Buffer.from(await image(name).arrayBuffer());
  const created = await author.optionalWrite<{ reference: string }>(
    "POST",
    "/chat/uploads",
    {
      uploadId: "76a2a3e6-02e9-429b-9be6-9da6d632803d",
      fileName: name,
      contentType: "image/png",
      base64: bytes.toString("base64"),
    },
    [410],
  );
  // The durable copy remains on the issue after the chat upload expires.
  return created?.reference ?? reference;
}
