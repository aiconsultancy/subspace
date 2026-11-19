const origin = new URL(process.env.SEED_API_URL ?? "http://127.0.0.1:3000");
if (!["127.0.0.1", "localhost", "[::1]"].includes(origin.hostname)) {
  throw new Error("Demo data requires a loopback API.");
}

type RequestOptions = {
  token?: string;
  body?: unknown;
  headers?: HeadersInit;
  allow?: number[];
};

export async function request<T>(
  method: string,
  path: string,
  { token, body, headers: inputHeaders, allow = [] }: RequestOptions = {},
): Promise<T | null> {
  const headers = new Headers(inputHeaders);
  if (token) headers.set("Authorization", `Bearer ${token}`);
  const multipart = body instanceof FormData;
  if (body !== undefined && !multipart) headers.set("Content-Type", "application/json");
  const response = await fetch(new URL(`/api/v1${path}`, origin), {
    method,
    headers,
    body: multipart ? body : JSON.stringify(body),
    redirect: "error",
    signal: AbortSignal.timeout(15_000),
  });
  if (allow.includes(response.status)) return null;
  if (!response.ok) {
    const problem = await response.text();
    throw new Error(`${method} ${path} returned ${response.status}: ${problem}`);
  }
  return response.status === 204 ? null : ((await response.json()) as T);
}

export function client(token?: string) {
  return {
    async get<T>(path: string): Promise<T> {
      return (await request<T>("GET", path, { token }))!;
    },
    async find<T>(path: string, allow = [404]): Promise<T | null> {
      return request<T>("GET", path, { token, allow });
    },
    async write<T>(method: string, path: string, body: unknown): Promise<T> {
      return (await request<T>(method, path, { token, body }))!;
    },
    async optionalWrite<T>(method: string, path: string, body: unknown, allow: number[]) {
      return request<T>(method, path, { token, body, allow });
    },
    async all<T>(path: string): Promise<T[]> {
      const items: T[] = [];
      let cursor: string | null = null;
      do {
        const query = `${path.includes("?") ? "&" : "?"}limit=100`;
        const page: { items: T[]; nextCursor: string | null } = (await request(
          "GET",
          `${path}${query}${cursor ? `&cursor=${encodeURIComponent(cursor)}` : ""}`,
          { token },
        ))!;
        items.push(...page.items);
        cursor = page.nextCursor;
      } while (cursor);
      return items;
    },
  };
}

export type Client = ReturnType<typeof client>;
export type Identified = { id: string };
export type Versioned = Identified & { version: number };
export type SeedIssue = Versioned & { identifier: string; title: string };
