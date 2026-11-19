// This factory runs at document start. Native IPC still verifies the source
// independently; the page's claims never authorize a call.
(() => {
  if (globalThis.self !== globalThis.top || location.origin !== "__SUBSPACE_APP_ORIGIN__") {
    return;
  }
  const document = Array.from(crypto.getRandomValues(new Uint8Array(16)),
    value => value.toString(16).padStart(2, "0")).join("");
  const pending = new Map();
  const post = globalThis.ipc.postMessage.bind(globalThis.ipc);
  let sequence = 0;
  let permission = "default";

  function call(method, value) {
    if (pending.size >= 32) {
      return Promise.reject(new Error("Too many pending Subspace requests."));
    }
    sequence += 1;
    const id = sequence;
    return new Promise((resolve, reject) => {
      // Serialize before reserving a slot. Cyclic values and BigInt reject
      // the promise without leaving an entry or timer behind.
      const message = JSON.stringify({ id, document, operation: { method, value } });
      if (new TextEncoder().encode(message).length > 16384) {
        reject(new Error("Subspace request is too large."));
        return;
      }
      const timer = setTimeout(() => {
        pending.delete(id);
        reject(new Error("Subspace request timed out."));
      }, 10000);
      pending.set(id, { resolve, reject, timer });
      try {
        post(message);
      } catch (error) {
        clearTimeout(timer);
        pending.delete(id);
        reject(error);
      }
    });
  }

  Object.defineProperty(globalThis, "__subspaceReply", { value: reply => {
    if (reply.document !== document) {
      return;
    }
    const request = pending.get(reply.id);
    if (!request) {
      return;
    }
    pending.delete(reply.id);
    clearTimeout(request.timer);
    if (reply.error) {
      request.reject(new Error(reply.error));
    } else {
      request.resolve(reply.value);
    }
  } });

  async function info() {
    const result = await call("info");
    permission = result.notificationPermission;
    return result;
  }

  Object.defineProperty(globalThis, "subspace", { value: Object.freeze({
    version: 1,
    info,
    settings: Object.freeze({ get: () => call("settings.get") }),
    setBadge: value => call("badge.set", value === null ? null : String(value)),
    setContext: value => call("context.set", value),
    notify: value => call("notify", value),
  }) });

  function sendNotification(notification) {
    call("notify", { title: notification.title, body: notification.body }).then(() => {
      notification.dispatchEvent(new Event("show"));
      notification.onshow?.(new Event("show"));
    }).catch(() => {
      notification.dispatchEvent(new Event("error"));
      notification.onerror?.(new Event("error"));
    });
  }

  // A foreground-page Notification adapter, not service-worker/Web Push
  // support. requestPermission reads native Settings; it never grants itself.
  class BoardNotification extends EventTarget {
    constructor(title, options = {}) {
      super();
      this.title = String(title);
      this.body = String(options.body ?? "");
      sendNotification(this);
    }
    static get permission() { return permission; }
    static async requestPermission(callback) {
      const result = await info();
      callback?.(result.notificationPermission);
      return result.notificationPermission;
    }
    close() {
      // Delivery belongs to the host notification center; page retraction is unsupported.
    }
  }
  Object.defineProperty(globalThis, "Notification", { value: BoardNotification, configurable: true });
  info().catch(() => { permission = "denied"; });
})();
