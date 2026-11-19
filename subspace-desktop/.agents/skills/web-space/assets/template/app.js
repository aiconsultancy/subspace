// Pure logic: no DOM, so Node tests can pass a fake bridge.
export async function greeting(subspace) {
  if (!subspace) return "Hello from a browser";
  const info = await subspace.info();
  if (!info.permissions.includes("settings")) return "Hello from Subspace";
  const settings = await subspace.settings.get();
  return `${settings.greeting ?? "Hello"} from Subspace`;
}
