/** Parse a loopback test database URL without accepting connection options. */
export function localDatabase(raw: string) {
  const url = new URL(raw);
  if (!["postgres:", "postgresql:"].includes(url.protocol) || url.search || url.hash)
    throw new Error("TEST_DATABASE_URL must be a postgresql:// URL without options.");
  if (!["localhost", "127.0.0.1", "[::1]"].includes(url.hostname))
    throw new Error("TEST_DATABASE_URL must point to a local test database.");
  const host = url.hostname === "[::1]" ? "::1" : url.hostname;
  const port = Number(url.port || "5432");
  const user = decodeURIComponent(url.username);
  const password = decodeURIComponent(url.password);
  const database = decodeURIComponent(url.pathname.slice(1)) || user;
  const settings = (name = database) => ({ host, port, user, password, database: name });
  const urlFor = (name: string) => {
    const clean = new URL(url.href);
    clean.pathname = `/${encodeURIComponent(name)}`;
    return clean.href;
  };
  return { database, settings, urlFor };
}
