// AgentsView 0.47 uses fetch for its API, including event streams. Install
// before its scripts start and keep the key out of URLs and browser storage.
(config => {
  const { origin, apiPath, key } = config;
  if (window.location.origin !== origin) return;

  const fetchPage = window.fetch.bind(window);
  const report = window.ipc.postMessage.bind(window.ipc);
  window.fetch = async (input, options) => {
    const url = new URL(input instanceof Request ? input.url : input, document.baseURI);
    if (url.origin !== origin || !url.pathname.startsWith(apiPath)) {
      return fetchPage(input, options);
    }
    const settings = url.pathname === apiPath + "v1/settings";
    const request = new Request(input, options);
    if (key) request.headers.set("Authorization", "Bearer " + key);
    const response = await fetchPage(request, { redirect: "error" }).catch(error => {
      if (settings) report("unavailable");
      throw error;
    });
    if (response.status === 401 || (settings && response.status === 403)) {
      report("rejected");
      // The embedded app would use a 401 to open its own key form.
      throw new Error("AgentsView authentication failed");
    }
    if (settings && request.method === "GET") {
      report(response.ok && response.headers.get("Content-Type")?.includes("application/json")
        ? "ready" : "unavailable");
    }
    return response;
  };
})(/*__SUBSPACE_AGENTSVIEW_CONFIG__*/);
