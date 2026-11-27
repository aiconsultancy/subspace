// The system monitor's single content pane. The host's workspace shell draws
// the Space's name, so this view has no heading of its own; text starts at
// the design guide's text edge (`--text-edge`, set by the host).
import { Scroll, Text, View, type Accessor, type JSX } from "nsh";
import { Graph } from "./chart";
import { createMonitor } from "./monitor";
import {
  bytes,
  clock,
  factRows,
  memoryPercent,
  percent,
  plural,
  rate,
  reading,
  SAMPLE_SECONDS,
  type AppUsage,
  type Tier,
} from "./model";

/** What each tier's empty state says it would show and which grant it needs. */
const GRANTS = {
  metrics: { what: "CPU, memory and network", grant: "System metrics" },
  apps: { what: "Top apps by memory", grant: "Running apps" },
  facts: { what: "System facts", grant: "System facts" },
} as const;

function Section(props: { title: string; children: JSX.Element; testID?: string }) {
  return (
    <View testID={props.testID} class="gap-2 border-b border-border py-3 px-(--text-edge)">
      <Text class="text-xs font-medium text-muted-foreground">{props.title}</Text>
      {props.children}
    </View>
  );
}

function Empty(props: { text: string; testID: string }) {
  return (
    <Text testID={props.testID} class="text-sm text-muted-foreground">
      {props.text}
    </Text>
  );
}

/** A tier's content once it has a reading, or an explained empty state. */
function TierView<T>(props: {
  name: keyof typeof GRANTS;
  tier: Accessor<Tier<T>>;
  children: (reading: () => T) => JSX.Element;
}) {
  const { what, grant } = GRANTS[props.name];
  const ready = () => {
    const tier = props.tier();
    return tier.kind === "ready" ? tier.reading : null;
  };
  const empty = () => {
    const tier = props.tier();
    switch (tier.kind) {
      case "refused":
        return `${what} need the “${grant}” permission. Grant it to this Space to show them here.`;
      case "standalone":
        return `${what} come from Subspace. Open this Space in Subspace to see them.`;
      case "failed":
        return `${what} are unavailable: ${tier.message}`;
      default:
        return "Waiting for the first sample…";
    }
  };
  return (
    <If when={ready()} else={<Empty testID={`empty-${props.name}`} text={empty()} />}>
      {(value) => props.children(value as () => T)}
    </If>
  );
}

function Metric(props: { label: string; value: string; testID?: string }) {
  return (
    <View class="flex-row items-baseline justify-between gap-2">
      <Text class="text-sm text-foreground">{props.label}</Text>
      <Text testID={props.testID} class="text-sm text-foreground tabular-nums">
        {props.value}
      </Text>
    </View>
  );
}

export function SystemMonitor() {
  const monitor = createMonitor();
  const history = monitor.history;

  return (
    <Scroll class="flex-1 bg-background">
      <TierView name="metrics" tier={monitor.metrics}>
        {(m) => (
          <View>
            <View class="py-2 px-(--text-edge)">
              <Text testID="sample-status" class="text-xs text-muted-foreground">
                {`Sampled every ${SAMPLE_SECONDS.metrics} s · last sample ${clock(m().at)}`}
              </Text>
            </View>
            <Section title="CPU" testID="section-cpu">
              <Metric label="Overall" testID="cpu-overall" value={reading(m().cpu, percent)} />
              <Graph testID="graph-cpu" label="CPU use, last 60 samples" series={() => [history().cpu]} colors={["chart-1"]} max={100} />
              <View class="flex-row flex-wrap gap-2">
                <Each of={history().cores}>
                  {(core, index) => (
                    <View class="w-[23%] min-w-20 gap-1">
                      <Text class="text-xs text-muted-foreground tabular-nums">
                        {`Core ${index() + 1} · ${reading(m().cores[index()], percent)}`}
                      </Text>
                      <Graph label={`Core ${index() + 1} use`} series={() => [core()]} colors={["chart-1"]} max={100} height={28} />
                    </View>
                  )}
                </Each>
              </View>
            </Section>
            <Section title="Memory" testID="section-memory">
              <Metric
                label="Used"
                testID="memory-used"
                value={
                  m().memory.used === null || m().memory.total === null
                    ? reading(null, bytes)
                    : `${bytes(m().memory.used!)} of ${bytes(m().memory.total!)} · ${reading(memoryPercent(m().memory), percent)}`
                }
              />
              <Metric
                label="Swap"
                value={m().memory.swapUsed === null ? reading(null, bytes) : `${bytes(m().memory.swapUsed!)} of ${reading(m().memory.swapTotal, bytes)}`}
              />
              <Graph testID="graph-memory" label="Memory use, last 60 samples" series={() => [history().memory]} colors={["chart-2"]} max={100} />
            </Section>
            <Section title="Network" testID="section-network">
              <Metric label="Down" testID="network-down" value={reading(m().network.rx, rate)} />
              <Metric label="Up" testID="network-up" value={reading(m().network.tx, rate)} />
              <Graph
                testID="graph-network"
                label="Network down and up, last 60 samples"
                series={() => [history().rx, history().tx]}
                colors={["chart-3", "chart-4"]}
                max="peak"
              />
            </Section>
          </View>
        )}
      </TierView>
      <Section title="Top apps by memory" testID="section-apps">
        <TierView name="apps" tier={monitor.apps}>
          {(apps) => (
            <View class="gap-1">
              <Text class="text-xs text-muted-foreground">
                {`Sampled every ${SAMPLE_SECONDS.apps} s · last sample ${clock(apps().at)}`}
              </Text>
              <Each of={apps().apps} key={(app: AppUsage) => app.id}>
                {(app) => (
                  <View class="flex-row items-baseline justify-between gap-2 py-1">
                    <Text class="flex-1 text-sm text-foreground truncate">
                      {app().name}
                    </Text>
                    <Text class="text-xs text-muted-foreground tabular-nums">{plural(app().processes, "process", "processes")}</Text>
                    <Text class="w-20 text-right text-sm text-foreground tabular-nums">{bytes(app().memory)}</Text>
                  </View>
                )}
              </Each>
            </View>
          )}
        </TierView>
      </Section>
      <Section title="System" testID="section-facts">
        <TierView name="facts" tier={monitor.facts}>
          {(facts) => (
            <View class="gap-1">
              <Each of={factRows(facts())} key={(row: [string, string]) => row[0]}>
                {(row) => (
                  <View class="flex-row gap-3">
                    <Text class="w-28 text-sm text-muted-foreground">{row()[0]}</Text>
                    <Text class="flex-1 text-sm text-foreground">{row()[1]}</Text>
                  </View>
                )}
              </Each>
            </View>
          )}
        </TierView>
      </Section>
    </Scroll>
  );
}
