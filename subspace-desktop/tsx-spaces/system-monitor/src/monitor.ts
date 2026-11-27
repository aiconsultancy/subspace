// Connects the view to the host's system tiers: the latest reading with
// `host.call`, then each new one the host sends on the same name.
import { host, signal, type Accessor } from "nsh";
import { emptyHistory, failure, HOST, record, type Apps, type Facts, type History, type Metrics, type Tier, type TierName } from "./model";

function connect<T>(name: TierName, onReading?: (reading: T) => void): Accessor<Tier<T>> {
  const state = signal<Tier<T>>({ kind: "loading" });
  const accept = (payload: unknown) => {
    const reading = payload as T;
    onReading?.(reading);
    state.set({ kind: "ready", reading });
  };
  host.on(HOST[name], accept);
  host.call(HOST[name]).then(accept, (error: unknown) => state.set(failure<T>(error)));
  return state;
}

export interface Monitor {
  metrics: Accessor<Tier<Metrics>>;
  history: Accessor<History>;
  apps: Accessor<Tier<Apps>>;
  facts: Accessor<Tier<Facts>>;
}

export function createMonitor(): Monitor {
  const history = signal<History>(emptyHistory);
  // A reading pushed and also returned by the first call is kept once.
  let last = -1;
  const metrics = connect<Metrics>("metrics", (reading) => {
    if (reading.at === last) return;
    last = reading.at;
    history.update((current) => record(current, reading));
  });
  return { metrics, history, apps: connect<Apps>("apps"), facts: connect<Facts>("facts") };
}
