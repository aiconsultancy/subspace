import { useEffect, useId, useRef, useState, type ComponentType, type CSSProperties, type KeyboardEvent, type ReactNode } from 'react';
import { Check, ChevronDown, ChevronLeft, ChevronRight, Copy, File, Folder, FolderOpen, Pencil, Plus, Trash } from 'lucide-react';
import { Button } from './ui/button';
import { Input } from './ui/input';
import './kit-regions.css';

// All data follows samples/kit/advanced/*.rs, including every chart series.
function Sample({ caption, children }: { caption: string; children: ReactNode }) {
  return <div className="kr-sample">{children}<div className="sample-caption">{caption}</div></div>;
}

function DiffSample() {
  const lines = [
    ['', '', '@@ -1,4 +1,5 @@', 'hunk'],
    ['1', '1', " fn fixture() -> &'static str {", ''],
    ['2', '', '-    "old value"', 'remove'],
    ['', '2', '+    // The gallery shows a fixture patch.', 'add'],
    ['', '3', '+    "new value"', 'add'],
    ['3', '4', ' }', ''], ['4', '5', ' ', ''],
  ];
  return <Sample caption="One file, one hunk"><div className="kr-data-region kr-diff" tabIndex={0} aria-label="Changes to src/fixture.rs">
    <header><File size={14} aria-hidden="true" /> src/fixture.rs <span className="kr-muted">+2 −1</span></header>
    <pre><code>{lines.map(([before, after, text, kind], ix) => <span key={ix} className={`kr-diff-line ${kind ? `kr-diff-${kind}` : ''}`}>
      <span aria-hidden="true">{before}</span><span aria-hidden="true">{after}</span><span>{text}</span>
    </span>)}</code></pre>
  </div></Sample>;
}

function CalendarSample() {
  const [month, setMonth] = useState(() => { const today = new Date(); return new Date(today.getFullYear(), today.getMonth(), 1); });
  const [selected, setSelected] = useState('');
  const calendar = useRef<HTMLDivElement>(null);
  const focusDate = useRef('');
  const heading = useId();
  const first = new Date(month.getFullYear(), month.getMonth(), 1 - month.getDay());
  const days = Array.from({ length: 42 }, (_, ix) => new Date(first.getFullYear(), first.getMonth(), first.getDate() + ix));
  const dateKey = (date: Date) => `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`;
  useEffect(() => {
    if (focusDate.current) calendar.current?.querySelector<HTMLButtonElement>(`[data-day="${focusDate.current}"]`)?.focus();
    focusDate.current = '';
  }, [month]);
  function moveDay(event: KeyboardEvent<HTMLButtonElement>, date: Date) {
    const deltas: Record<string, number> = { ArrowLeft: -1, ArrowRight: 1, ArrowUp: -7, ArrowDown: 7 };
    const delta = deltas[event.key];
    if (!delta) return;
    event.preventDefault();
    const next = new Date(date.getFullYear(), date.getMonth(), date.getDate() + delta);
    const key = dateKey(next);
    const target = calendar.current?.querySelector<HTMLButtonElement>(`[data-day="${key}"]`);
    if (target) target.focus();
    else { focusDate.current = key; setMonth(new Date(next.getFullYear(), next.getMonth(), 1)); }
  }
  return <Sample caption="Click a day"><div className="kr-calendar" ref={calendar}>
    <header><Button variant="ghost" aria-label="Previous month" onClick={() => setMonth(new Date(month.getFullYear(), month.getMonth() - 1, 1))}><ChevronLeft /></Button>
      <strong id={heading}>{month.toLocaleDateString('en-GB', { month: 'long', year: 'numeric' })}</strong>
      <Button variant="ghost" aria-label="Next month" onClick={() => setMonth(new Date(month.getFullYear(), month.getMonth() + 1, 1))}><ChevronRight /></Button></header>
    <div className="kr-calendar-grid" role="group" aria-labelledby={heading}>
      {['Su', 'Mo', 'Tu', 'We', 'Th', 'Fr', 'Sa'].map(day => <span key={day}>{day}</span>)}
      {days.map(date => <Button key={dateKey(date)} data-day={dateKey(date)} aria-label={date.toLocaleDateString('en-GB', { dateStyle: 'full' })}
        aria-pressed={selected === dateKey(date)} variant={selected === dateKey(date) ? 'selected' : 'ghost'} className={date.getMonth() !== month.getMonth() ? 'kr-other-month' : ''}
        onKeyDown={event => moveDay(event, date)} onClick={() => setSelected(dateKey(date))}>{date.getDate()}</Button>)}
    </div>
    {selected && <output className="kr-muted">Selected: {selected}</output>}
  </div></Sample>;
}

function CarouselSample() {
  const [slide, setSlide] = useState(0);
  const slides = ['First slide', 'Second slide', 'Third slide'];
  return <Sample caption="Previous and next"><section className="kr-carousel" aria-label="Fixture slides" aria-roledescription="carousel">
    <div className="kr-slide" aria-live="polite" aria-roledescription="slide" aria-label={`${slide + 1} of 3`}>{slides[slide]}</div>
    <div className="kr-carousel-controls"><Button variant="outline" aria-label="Previous slide" disabled={slide === 0} onClick={() => setSlide(slide - 1)}><ChevronLeft /></Button>
      <span className="kr-muted">{slide + 1} / 3</span><Button variant="outline" aria-label="Next slide" disabled={slide === 2} onClick={() => setSlide(slide + 1)}><ChevronRight /></Button></div>
  </section></Sample>;
}

const COMMANDS = [
  { label: 'New fixture', group: 'Fixtures', Icon: Plus, disabled: false },
  { label: 'Open fixture', group: 'Fixtures', Icon: FolderOpen, disabled: false },
  { label: 'Delete fixture', group: 'Fixtures', Icon: Trash, disabled: true },
  { label: 'Copy a name', group: 'Gallery', Icon: Copy, disabled: false },
];
function CommandSample() {
  const [query, setQuery] = useState('');
  const [active, setActive] = useState('New fixture');
  const [result, setResult] = useState('');
  const id = useId();
  const filtered = COMMANDS.filter(command => command.label.toLowerCase().includes(query.toLowerCase()));
  const enabled = filtered.filter(command => !command.disabled);
  function navigate(event: KeyboardEvent<HTMLInputElement>) {
    const ix = enabled.findIndex(command => command.label === active);
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') { event.preventDefault(); setActive(enabled[(ix + (event.key === 'ArrowDown' ? 1 : enabled.length - 1)) % enabled.length]?.label ?? ''); }
    if (event.key === 'Home' || event.key === 'End') { event.preventDefault(); setActive((event.key === 'Home' ? enabled[0] : enabled.at(-1))?.label ?? ''); }
    if (event.key === 'Enter' && enabled.some(command => command.label === active)) { event.preventDefault(); setResult(`${active} selected`); }
    if (event.key === 'Escape') setQuery('');
  }
  return <Sample caption="Filter as you type"><div className="kr-data-region kr-narrow kr-command">
    <Input aria-label="Type a command" role="combobox" aria-expanded="true" aria-controls={id} aria-autocomplete="list"
      aria-activedescendant={filtered.some(command => command.label === active) ? `${id}-${COMMANDS.findIndex(command => command.label === active)}` : undefined}
      placeholder="Type a command…" value={query} onChange={event => { setQuery(event.target.value); setActive(COMMANDS.find(command => !command.disabled && command.label.toLowerCase().includes(event.target.value.toLowerCase()))?.label ?? ''); }} onKeyDown={navigate} />
    <div className="kr-command-results" role="listbox" id={id} aria-label="Commands">
      {['Fixtures', 'Gallery'].map(group => <div key={group} role="group" aria-label={group}>
        {filtered.some(command => command.group === group) && <div className="kr-command-group">{group}</div>}
        {filtered.filter(command => command.group === group).map(({ label, Icon, disabled }) => <Button key={label} id={`${id}-${COMMANDS.findIndex(command => command.label === label)}`} variant={active === label ? 'selected' : 'ghost'}
          role="option" aria-selected={active === label} aria-disabled={disabled} disabled={disabled} tabIndex={-1} onMouseEnter={() => { if (!disabled) setActive(label); }} onClick={() => setResult(`${label} selected`)}><Icon />{label}</Button>)}
      </div>)}
      {!filtered.length && <p className="kr-muted">No commands found.</p>}
    </div>
  </div>{result && <output>{result}</output>}</Sample>;
}

const MONTHS = [['Jan', 186, 80], ['Feb', 305, 200], ['Mar', 237, 120], ['Apr', 73, 190], ['May', 209, 130], ['Jun', 214, 140]] as const;
const SKILLS = [['Speed', 80], ['Range', 65], ['Grip', 90], ['Weight', 40], ['Cost', 70]] as const;
const PRICES = [['Mon', 10, 14, 9, 13], ['Tue', 13, 15, 11, 12], ['Wed', 12, 16, 12, 15], ['Thu', 15, 17, 13, 14], ['Fri', 14, 18, 14, 17]] as const;
const FLOW = [['Visits', 'Sign-ups', 60], ['Sign-ups', 'Trials', 40], ['Trials', 'Purchases', 25]] as const;
const SPARK = [[0, .8], [.2, .4], [.4, .6], [.6, .2], [.8, .5], [1, .1]] as const;
const PIE_COLOURS = ['var(--primary-background)', 'var(--success-background)', 'var(--tone-orange, var(--warning-background))', 'var(--base-blue)', 'var(--danger-background)', 'var(--base-cyan)'];

function ChartFrame({ name, children, rows, headers = ['Month', 'Series 1', 'Series 2'] }: { name: string; children: ReactNode; rows: ReadonlyArray<ReadonlyArray<string | number>>; headers?: string[] }) {
  return <Sample caption={name}><svg className="kr-chart" viewBox="0 0 300 180" role="img" aria-label={name}>
    <title>{name} — fixture data</title>{children}
  </svg><details className="kr-chart-data"><summary>View {name} data</summary><table><thead><tr>{headers.map(header => <th key={header}>{header}</th>)}</tr></thead><tbody>
    {rows.map((row, ix) => <tr key={ix}>{row.map((cell, col) => <td key={col}>{cell}</td>)}</tr>)}
  </tbody></table></details></Sample>;
}

function ChartAxes() {
  return <g><path className="kr-axis" d="M32 12V150H292" fill="none" />
    {[0, 100, 200, 300].map(value => <g key={value}><line className="kr-axis" x1={32} x2={292} y1={150 - value * .4} y2={150 - value * .4} /><text x={26} y={154 - value * .4} textAnchor="end">{value}</text></g>)}
    {MONTHS.map(([label], ix) => <text key={label} x={54 + ix * 44} y={170} textAnchor="middle">{label}</text>)}
  </g>;
}

function CartesianChart({ kind }: { kind: 'bar' | 'line' | 'area' }) {
  const first = MONTHS.map(([, value], ix) => `${54 + ix * 44},${150 - value * .4}`).join(' ');
  const second = MONTHS.map(([, , value], ix) => `${54 + ix * 44},${150 - value * .4}`).join(' ');
  return <ChartFrame name={kind === 'bar' ? 'BarChart' : kind === 'line' ? 'LineChart' : 'AreaChart, two series'} rows={MONTHS}>
    <ChartAxes />
    {kind === 'bar' ? MONTHS.map(([label, value], ix) => <rect key={label} x={40 + ix * 44} y={150 - value * .4} width={28} height={value * .4} fill="var(--primary-background)"><title>{label}: {value}</title></rect>) : <>
      {kind === 'area' && <><polygon points={`54,150 ${first} 274,150`} fill="var(--primary-background)" opacity=".18" /><polygon points={`54,150 ${second} 274,150`} fill="var(--success-background)" opacity=".18" /><polyline points={second} className="kr-series-b" /></>}
      <polyline points={first} className="kr-series-a" />
      {kind === 'line' && MONTHS.map(([label, value], ix) => <circle key={label} cx={54 + ix * 44} cy={150 - value * .4} r={3} fill="var(--primary-background)"><title>{label}: {value}</title></circle>)}
    </>}
  </ChartFrame>;
}

function PiePair() {
  const totalA = MONTHS.reduce((sum, month) => sum + month[1], 0);
  const totalB = MONTHS.reduce((sum, month) => sum + month[2], 0);
  function slices(series: 1 | 2, centre: number, radius: number) {
    let angle = -Math.PI / 2;
    return MONTHS.map((month, ix) => {
      const next = angle + month[series] / (series === 1 ? totalA : totalB) * Math.PI * 2;
      const startX = centre + radius * Math.cos(angle), startY = 90 + radius * Math.sin(angle);
      const endX = centre + radius * Math.cos(next), endY = 90 + radius * Math.sin(next);
      const path = `M${centre},90 L${startX},${startY} A${radius},${radius} 0 ${next - angle > Math.PI ? 1 : 0},1 ${endX},${endY} Z`;
      angle = next;
      return <path key={month[0]} d={path} fill={PIE_COLOURS[ix]}><title>{month[0]}: {month[series]}</title></path>;
    });
  }
  return <ChartFrame name="PieChart and donut" rows={MONTHS}>
    {slices(1, 75, 68)}{slices(2, 225, 68)}<circle cx={225} cy={90} r={39} fill="var(--muted-background)" />
  </ChartFrame>;
}

function RadarSample() {
  const point = (ix: number, value: number) => { const angle = ix * Math.PI * 2 / 5 - Math.PI / 2; return [150 + Math.cos(angle) * value * .6, 90 + Math.sin(angle) * value * .6]; };
  return <ChartFrame name="RadarChart" rows={SKILLS} headers={['Skill', 'Value / 100']}>
    {[25, 50, 75, 100].map(value => <polygon key={value} points={SKILLS.map((_, ix) => point(ix, value).join(',')).join(' ')} className="kr-axis" fill="none" />)}
    {SKILLS.map(([label], ix) => { const [x, y] = point(ix, 125); const [endX, endY] = point(ix, 100); return <g key={label}><line x1={150} y1={90} x2={endX} y2={endY} className="kr-axis" /><text x={x} y={y} textAnchor="middle">{label}</text></g>; })}
    <polygon points={SKILLS.map(([, value], ix) => point(ix, value).join(',')).join(' ')} stroke="var(--primary-background)" fill="var(--primary-background)" fillOpacity=".2" strokeWidth="2" />
  </ChartFrame>;
}

function CandlestickSample() {
  const y = (value: number) => 150 - (value - 8) * 12;
  return <ChartFrame name="CandlestickChart" rows={PRICES} headers={['Day', 'Open', 'High', 'Low', 'Close']}>
    <path className="kr-axis" d="M24 16V150H292" fill="none" />
    {[10, 12, 14, 16, 18].map(value => <text key={value} x={19} y={y(value) + 4} textAnchor="end">{value}</text>)}
    {PRICES.map(([day, open, high, low, close], ix) => <g key={day}>
      <line x1={54 + ix * 48} x2={54 + ix * 48} y1={y(high)} y2={y(low)} stroke={close >= open ? 'var(--success-background)' : 'var(--danger-background)'} />
      <rect x={44 + ix * 48} y={y(Math.max(open, close))} width={20} height={Math.abs(close - open) * 12} fill={close >= open ? 'var(--success-background)' : 'var(--danger-background)'}><title>{day}: open {open}, high {high}, low {low}, close {close}</title></rect>
      <text x={54 + ix * 48} y={170} textAnchor="middle">{day}</text>
    </g>)}
  </ChartFrame>;
}

function SankeySample() {
  const nodes = ['Visits', 'Sign-ups', 'Trials', 'Purchases'];
  return <ChartFrame name="SankeyChart" rows={FLOW} headers={['From', 'To', 'Value']}>
    {FLOW.map(([from, to, value], ix) => <path key={from} d={`M${28 + ix * 80},90 C${60 + ix * 80},90 ${60 + ix * 80},90 ${96 + ix * 80},90`} stroke="var(--primary-background)" strokeWidth={value} opacity=".25" fill="none"><title>{from} → {to}: {value}</title></path>)}
    {nodes.map((node, ix) => <g key={node}><rect x={16 + ix * 80} y={90 - [60, 60, 40, 25][ix] / 2} width={12} height={[60, 60, 40, 25][ix]} fill="var(--primary-background)" /><text x={22 + ix * 80} y={145} textAnchor="middle">{node}</text></g>)}
  </ChartFrame>;
}

function ChartsSample() {
  return <div className="kr-chart-grid"><CartesianChart kind="bar" /><CartesianChart kind="line" /><CartesianChart kind="area" /><PiePair /><RadarSample /><CandlestickSample /><SankeySample /></div>;
}

function PlotSample() {
  return <ChartFrame name="A custom Plot that paints one line" rows={SPARK} headers={['Width fraction', 'Height fraction']}>
    <polyline points={SPARK.map(([x, y]) => `${x * 300},${y * 180}`).join(' ')} className="kr-series-a" />
  </ChartFrame>;
}

const FRUIT = ['Apple', 'Banana', 'Cherry', 'Date', 'Elderberry', 'Fig'];
function ListSample() {
  const [query, setQuery] = useState('');
  const [selected, setSelected] = useState('');
  const list = useRef<HTMLSelectElement>(null);
  const fruits = FRUIT.filter(fruit => fruit.toLowerCase().includes(query.toLowerCase()));
  useEffect(() => {
    // Native select otherwise visually selects its first option when the
    // model has no visible selection, swallowing Home on a single result.
    if (list.current && !fruits.includes(selected)) list.current.selectedIndex = -1;
  }, [fruits, selected]);
  return <Sample caption="Searchable; click or use the arrow keys"><div className="kr-data-region kr-narrow kr-list">
    <Input aria-label="Search fruit" placeholder="Search…" value={query} onChange={event => setQuery(event.target.value)} />
    <select ref={list} size={6} aria-label="Fixture fruit" value={selected} onChange={event => setSelected(event.target.value)}>
      {fruits.map(fruit => <option key={fruit} value={fruit}>{fruit}</option>)}
    </select>{!fruits.length && <span className="kr-muted">No fruit found.</span>}
  </div>{selected && <output>Selected: {selected}</output>}</Sample>;
}

function FixtureMenu({ native = false }: { native?: boolean }) {
  const menu = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const opener = useRef<HTMLButtonElement | null>(null);
  const [open, setOpen] = useState(false);
  const [pinned, setPinned] = useState(true);
  const [result, setResult] = useState('');
  const id = useId();
  useEffect(() => { if (open) menu.current?.querySelector<HTMLButtonElement>('button')?.focus(); }, [open]);
  useEffect(() => {
    if (!open) return;
    const close = (event: PointerEvent) => { if (!menu.current?.contains(event.target as Node) && !trigger.current?.contains(event.target as Node)) setOpen(false); };
    document.addEventListener('pointerdown', close);
    return () => document.removeEventListener('pointerdown', close);
  }, [open]);
  function show(target: HTMLButtonElement) { opener.current = target; setOpen(true); }
  function dismiss() { setOpen(false); opener.current?.focus(); }
  function run(label: string) { setResult(`${label} selected`); dismiss(); }
  function keyboard(event: KeyboardEvent<HTMLDivElement>) {
    const options = Array.from(menu.current?.querySelectorAll<HTMLButtonElement>('button:not(:disabled)') ?? []);
    const ix = options.indexOf(document.activeElement as HTMLButtonElement);
    if (event.key === 'Escape' || event.key === 'Tab') { if (event.key === 'Escape') event.preventDefault(); dismiss(); }
    if (['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) {
      event.preventDefault();
      const next = event.key === 'Home' ? 0 : event.key === 'End' ? options.length - 1 : (ix + (event.key === 'ArrowDown' ? 1 : options.length - 1)) % options.length;
      options[next]?.focus();
    }
  }
  return <div className="kr-sample kr-menu-host"><div className="kr-row"><Sample caption={native ? "The operating system's own menu" : 'Dropdown, through anchored::dropdown_menu'}>
    <Button ref={trigger} variant="outline" aria-haspopup="menu" aria-expanded={open} aria-controls={id} onClick={event => { if (open) dismiss(); else show(event.currentTarget); }} onKeyDown={event => { if (event.key === 'ArrowDown') { event.preventDefault(); show(event.currentTarget); } }}>
      {native ? 'Show native menu' : <><span>Actions</span><ChevronDown /></>}
    </Button>
  </Sample>{!native && <Sample caption="Context menu, through anchored"><Button variant="ghost" className="kr-context-target" onContextMenu={event => { event.preventDefault(); show(event.currentTarget); }}
    onClick={event => show(event.currentTarget)} onKeyDown={event => { if (event.key === 'ContextMenu' || (event.shiftKey && event.key === 'F10')) { event.preventDefault(); show(event.currentTarget); } }}>Right-click here</Button></Sample>}</div>
    {open && <div ref={menu} id={id} className="kr-menu" role="menu" aria-label={native ? 'Fixture native-menu equivalent' : 'Fixture actions'} onKeyDown={keyboard}>
      {native ? <><Button role="menuitem" variant="ghost" onClick={() => run('Fixture command')}>Fixture command</Button><hr role="separator" /><Button role="menuitemcheckbox" aria-checked={pinned} variant="ghost" onClick={() => { setPinned(!pinned); run('Checked command'); }}>{pinned && <Check />}Checked command</Button></> : <>
        <Button role="menuitem" variant="ghost" onClick={() => run('Rename fixture')}><Pencil />Rename…</Button>
        <Button role="menuitem" variant="ghost" onClick={() => run('Duplicate fixture')}><Copy />Duplicate</Button>
        <Button role="menuitemcheckbox" aria-checked={pinned} variant="ghost" onClick={() => { setPinned(!pinned); run('Pinned'); }}>{pinned && <Check />}Pinned</Button><hr role="separator" />
        <Button role="menuitem" variant="ghost" disabled><Trash />Delete</Button>
      </>}
    </div>}
    {native && <p className="kr-native-note">Browser equivalent: these fixture commands use a DOM menu. A web Space cannot open the operating system’s native menu.</p>}
    {result && <output className="kr-muted">{result}</output>}
  </div>;
}
function PopupMenuSample() { return <FixtureMenu />; }
function NativeMenuSample() { return <FixtureMenu native />; }

function SettingsSample() {
  const [notify, setNotify] = useState(false);
  const [compact, setCompact] = useState(false);
  return <Sample caption="One page, one group"><div className="kr-data-region kr-settings">
    <nav aria-label="Settings pages"><Button variant="selected" aria-current="page">General</Button></nav>
    <section className="kr-settings-content" aria-label="General"><h3>Fixture</h3>
      <label className="kr-setting-row"><span>Notifications<small>Show a notice when a fixture changes.</small></span><input type="checkbox" role="switch" className="kr-switch" checked={notify} onChange={event => setNotify(event.target.checked)} /></label>
      <label className="kr-setting-row"><span>Compact rows<small>{compact ? 'Rows are compact.' : 'Rows are comfortable.'}</small></span><input type="checkbox" role="switch" className="kr-switch" checked={compact} onChange={event => setCompact(event.target.checked)} /></label>
    </section>
  </div></Sample>;
}

const ROWS = [
  ['Sample fixture', 'github', 12], ['Another fixture', 'actions', 7], ['Old fixture', 'irc', 3], ['New fixture', 'issues', 21],
  ['Large fixture', 'space-lab', 40], ['Small fixture', 'web', 1], ['Shared fixture', 'terminal', 9], ['Last fixture', 'component-gallery', 5],
] as const;
function FixtureTable({ sortable = false }: { sortable?: boolean }) {
  const [sort, setSort] = useState<{ column: number; descending: boolean } | null>(null);
  const rows = sortable ? [...ROWS].sort((a, b) => {
    if (!sort) return 0;
    const left = a[sort.column], right = b[sort.column];
    const order = typeof left === 'number' && typeof right === 'number' ? left - right : String(left).localeCompare(String(right));
    return order * (sort.descending ? -1 : 1);
  }) : ROWS.slice(0, 3);
  return <Sample caption={sortable ? 'Click a header to sort' : 'Header and three rows'}><div className="kr-data-region kr-table-scroll" tabIndex={0}>
    <table className="kr-table" aria-label={sortable ? 'Sortable fixtures' : 'Fixture table'}><thead><tr>
      {['Name', 'Board', 'Count'].map((label, column) => <th key={label} scope="col" aria-sort={sortable ? sort?.column === column ? sort.descending ? 'descending' : 'ascending' : 'none' : undefined}>
        {sortable ? <Button variant="ghost" onClick={() => setSort({ column, descending: sort?.column === column && !sort.descending })}>{label}<span aria-hidden="true">{sort?.column === column ? sort.descending ? ' ↓' : ' ↑' : ' ↕'}</span></Button> : label}
      </th>)}
    </tr></thead><tbody>{rows.map(row => <tr key={row[0]}>{row.map((cell, ix) => <td key={ix}>{cell}</td>)}</tr>)}</tbody></table>
  </div></Sample>;
}
function DataTableSample() { return <FixtureTable sortable />; }
function TableSample() { return <FixtureTable />; }

const PANELS: Record<string, string> = {
  Files: 'Drag this tab into the other group.', Search: 'Two panels share one tab group.', Editor: 'Drop a tab near an edge to split the group.',
};
function DockSample() {
  const [groups, setGroups] = useState([['Files', 'Search'], ['Editor']]);
  const [active, setActive] = useState(['Files', 'Editor']);
  const dragging = useRef('');
  const id = useId();
  function move(panel: string, target: number) {
    if (!panel || groups[target].includes(panel)) return;
    const next = groups.map((group, ix) => ix === target ? [...group, panel] : group.filter(name => name !== panel));
    setGroups(next); setActive(next.map((group, ix) => ix === target ? panel : group.includes(active[ix]) ? active[ix] : group[0] ?? ''));
  }
  return <Sample caption="Tabs and a split; drag a tab"><div className="kr-dock">
    {groups.map((group, ix) => <section key={ix} className="kr-dock-pane" aria-label={`Dock group ${ix + 1}`} onDragOver={event => event.preventDefault()} onDrop={event => { event.preventDefault(); move(dragging.current, ix); dragging.current = ''; }}>
      <div className="kr-tabs" role="tablist" aria-label={`Panel group ${ix + 1}`}>{group.map(panel => <Button key={panel} role="tab" id={`${id}-${panel}`} aria-controls={`${id}-body-${ix}`} aria-selected={active[ix] === panel} variant="ghost" draggable
        onDragStart={event => { dragging.current = panel; event.dataTransfer.setData('text/plain', panel); }}
        onClick={() => setActive(active.map((name, at) => at === ix ? panel : name))}
        onKeyDown={event => { if (event.key === 'ArrowRight' || event.key === 'ArrowLeft') { event.preventDefault(); const next = group[(group.indexOf(panel) + (event.key === 'ArrowRight' ? 1 : group.length - 1)) % group.length]; setActive(active.map((name, at) => at === ix ? next : name)); document.getElementById(`${id}-${next}`)?.focus(); } }}>{panel}</Button>)}</div>
      <div className="kr-dock-body" id={`${id}-body-${ix}`} role="tabpanel" aria-labelledby={active[ix] ? `${id}-${active[ix]}` : undefined} tabIndex={0}>{PANELS[active[ix]] ?? 'Drop a tab here.'}</div>
      {active[ix] && <Button variant="outline" className="kr-dock-move" onClick={() => move(active[ix], 1 - ix)}>Move {active[ix]} to other group</Button>}
    </section>)}
  </div><small className="kr-muted">Drag between the two groups, or use Move. Browser fixture keeps the native starting split.</small></Sample>;
}

function GridLayoutSample() {
  const [tiles, setTiles] = useState(['Revenue', 'Orders', 'Notes']);
  const [widths, setWidths] = useState([4, 2, 6]);
  const [heights, setHeights] = useState([2, 2, 1]);
  const dragging = useRef(-1);
  const resizing = useRef<{ x: number; y: number; width: number; height: number } | null>(null);
  function swap(from: number, target: number) { if (from < 0 || from === target) return; setTiles(previous => { const next = [...previous]; [next[from], next[target]] = [next[target], next[from]]; return next; }); }
  function resize(ix: number, width: number, height: number) {
    setWidths(previous => previous.map((value, at) => at === ix ? Math.max(1, Math.min([4, 2, 6][ix], width)) : value));
    setHeights(previous => previous.map((value, at) => at === ix ? Math.max(1, Math.min(3, height)) : value));
  }
  return <Sample caption="Drag or resize a tile"><div className="kr-grid">
    {tiles.map((tile, ix) => <div className="kr-tile" key={tile} style={{ gridColumn: `${ix === 1 ? 5 : 1} / span ${widths[ix]}`, gridRow: `${ix === 2 ? Math.max(heights[0], heights[1]) + 1 : 1} / span ${heights[ix]}`, resize: 'none' }}
      onDragOver={event => event.preventDefault()} onDrop={event => { event.preventDefault(); swap(dragging.current, ix); dragging.current = -1; }}>
      <Button variant="ghost" draggable onDragStart={event => { dragging.current = ix; event.dataTransfer.setData('text/plain', tile); }} aria-label={`Move ${tile}`} onKeyDown={event => { if (event.altKey && ['ArrowLeft', 'ArrowRight'].includes(event.key)) { event.preventDefault(); swap(ix, (ix + (event.key === 'ArrowRight' ? 1 : 2)) % 3); } }}>{tile}</Button>
      <div className="kr-tile-tools"><Button variant="ghost" aria-label={`Resize ${tile}; use arrow keys or drag`} title="Resize: arrow keys or drag" onKeyDown={event => {
        if (['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(event.key)) { event.preventDefault(); resize(ix, widths[ix] + (event.key === 'ArrowRight' ? 1 : event.key === 'ArrowLeft' ? -1 : 0), heights[ix] + (event.key === 'ArrowDown' ? 1 : event.key === 'ArrowUp' ? -1 : 0)); }
      }} onPointerDown={event => { event.currentTarget.setPointerCapture(event.pointerId); resizing.current = { x: event.clientX, y: event.clientY, width: widths[ix], height: heights[ix] }; }}
        onPointerMove={event => { const start = resizing.current; if (start && event.currentTarget.hasPointerCapture(event.pointerId)) { const grid = event.currentTarget.closest('.kr-grid'); const cell = (grid?.getBoundingClientRect().width ?? 520) / 6; resize(ix, start.width + Math.round((event.clientX - start.x) / cell), start.height + Math.round((event.clientY - start.y) / 56)); } }}
        onPointerUp={event => { event.currentTarget.releasePointerCapture(event.pointerId); resizing.current = null; }}>↘</Button></div>
    </div>)}
  </div><small className="kr-muted">Alt+Left/Right moves a focused tile. Resize handle accepts arrow keys.</small></Sample>;
}

function TabBarSample() {
  const [selected, setSelected] = useState(0);
  const id = useId();
  const labels = ['Overview', 'Activity', 'Settings'];
  return <div className="kr-row">{['Default', 'Underline', 'Pill', 'Outline', 'Segmented', 'Disabled tab'].map(style => {
    const tabs = style === 'Disabled tab' ? ['Enabled', 'Disabled'] : labels;
    const current = style === 'Disabled tab' ? 0 : selected;
    return <Sample key={style} caption={style}><div className="kr-tabs-example">
      <div className={`kr-tabs kr-tabs-${style.toLowerCase()}`} role="tablist" aria-label={`${style} tabs`}>
        {tabs.map((label, ix) => <Button key={label} id={`${id}-${style}-${ix}`} role="tab" aria-selected={current === ix} aria-controls={`${id}-${style}-panel`} tabIndex={current === ix ? 0 : -1}
          variant="ghost" disabled={style === 'Disabled tab' && ix === 1} onClick={() => { if (style !== 'Disabled tab') setSelected(ix); }}
          onKeyDown={event => {
            if (style === 'Disabled tab' || !['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return;
            event.preventDefault(); const next = event.key === 'Home' ? 0 : event.key === 'End' ? 2 : (ix + (event.key === 'ArrowRight' ? 1 : 2)) % 3;
            setSelected(next); document.getElementById(`${id}-${style}-${next}`)?.focus();
          }}>{label}</Button>)}
      </div><div className="kr-tabs-panel" role="tabpanel" id={`${id}-${style}-panel`} aria-labelledby={`${id}-${style}-${current}`}>{tabs[current]}</div>
    </div></Sample>;
  })}</div>;
}

function TreeSample() {
  const [expanded, setExpanded] = useState(['src']);
  const [selected, setSelected] = useState('');
  const [focused, setFocused] = useState('src');
  const tree = useRef<HTMLDivElement>(null);
  const folders: Record<string, string[]> = { src: ['lib.rs', 'view.rs'], docs: ['README.md'] };
  const toggle = (folder: string) => setExpanded(previous => previous.includes(folder) ? previous.filter(value => value !== folder) : [...previous, folder]);
  function navigation(event: KeyboardEvent<HTMLDivElement>) {
    const nodes = Array.from(tree.current?.querySelectorAll<HTMLButtonElement>('[role=treeitem]') ?? []);
    const target = event.target as HTMLButtonElement;
    const ix = nodes.indexOf(target);
    if (['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) { event.preventDefault(); nodes[event.key === 'Home' ? 0 : event.key === 'End' ? nodes.length - 1 : Math.max(0, Math.min(nodes.length - 1, ix + (event.key === 'ArrowDown' ? 1 : -1)))]?.focus(); }
    if (event.key === 'ArrowRight' && folders[target.dataset.path ?? '']) { event.preventDefault(); if (!expanded.includes(target.dataset.path!)) toggle(target.dataset.path!); else nodes[ix + 1]?.focus(); }
    if (event.key === 'ArrowLeft') { event.preventDefault(); const path = target.dataset.path ?? ''; if (expanded.includes(path)) toggle(path); else if (path.includes('/')) tree.current?.querySelector<HTMLButtonElement>(`[data-path="${path.split('/')[0]}"]`)?.focus(); }
  }
  function leaf(path: string, label: string, folder: boolean) {
    return <Button key={path} role="treeitem" data-path={path} tabIndex={focused === path ? 0 : -1} aria-level={path.includes('/') ? 2 : 1} aria-selected={selected === path} aria-expanded={folder ? expanded.includes(path) : undefined}
      variant={selected === path ? 'selected' : 'ghost'} className={path.includes('/') ? 'kr-tree-child' : ''} onFocus={() => setFocused(path)} onClick={() => { setSelected(path); if (folder) toggle(path); }}>
      {folder ? <><ChevronRight style={{ transform: expanded.includes(path) ? 'rotate(90deg)' : undefined }} /><Folder /></> : <File />}{label}
    </Button>;
  }
  return <Sample caption="Click a folder to expand it"><div className="kr-data-region kr-narrow kr-tree" role="tree" aria-label="Fixture files" ref={tree} onKeyDown={navigation}>
    {Object.entries(folders).map(([folder, children]) => <div key={folder} role="none">{leaf(folder, folder, true)}
      {expanded.includes(folder) && <div role="group">{children.map(name => leaf(`${folder}/${name}`, name, false))}</div>}
    </div>)}{leaf('Cargo.toml', 'Cargo.toml', false)}
  </div></Sample>;
}

function VirtualListSample() {
  const list = useRef<HTMLDivElement>(null);
  const [metrics, setMetrics] = useState({ height: 220, row: 28 });
  const [scrollTop, setScrollTop] = useState(0);
  useEffect(() => {
    const element = list.current;
    if (!element) return;
    const measure = () => setMetrics({ height: element.clientHeight, row: parseFloat(getComputedStyle(document.documentElement).fontSize) * 2 });
    const observer = new ResizeObserver(measure); observer.observe(element); measure();
    return () => observer.disconnect();
  }, []);
  const start = Math.max(0, Math.floor(scrollTop / metrics.row) - 2);
  const end = Math.min(1000, Math.ceil((scrollTop + metrics.height) / metrics.row) + 2);
  return <Sample caption="A thousand rows; only visible ones render"><div ref={list} className="kr-data-region kr-narrow kr-virtual" role="list" tabIndex={0} aria-label="1000 fixture rows"
    onScroll={event => setScrollTop(event.currentTarget.scrollTop)}>
    <div style={{ height: 1000 * metrics.row, position: 'relative' } as CSSProperties}>
      {Array.from({ length: end - start }, (_, offset) => { const row = start + offset; return <div key={row} className="kr-virtual-row" role="listitem" aria-posinset={row + 1} aria-setsize={1000} style={{ height: metrics.row, top: row * metrics.row }}>Fixture row {row}</div>; })}
    </div>
  </div></Sample>;
}

export const kitAdvancedSamples: Record<string, ComponentType> = {
  'gpui_kit::component::diff::Diff': DiffSample,
  'gpui_kit::component::calendar::Calendar': CalendarSample,
  'gpui_kit::component::carousel::Carousel': CarouselSample,
  'gpui_kit::component::command::Command': CommandSample,
  'gpui_kit::component::chart::BarChart': ChartsSample,
  'gpui_kit::component::plot::Plot': PlotSample,
  'gpui_kit::component::list::List': ListSample,
  'gpui_kit::component::menu::PopupMenu': PopupMenuSample,
  'gpui_kit::component::native_menu::NativeMenu': NativeMenuSample,
  'gpui_kit::component::setting::Settings': SettingsSample,
  'gpui_kit::component::table::DataTable': DataTableSample,
  'gpui_kit::component::table::Table': TableSample,
  'gpui_kit::component::dock::DockArea': DockSample,
  'gpui_kit::component::grid_layout::GridLayoutState': GridLayoutSample,
  'gpui_kit::component::tab::TabBar': TabBarSample,
  'gpui_kit::component::tree::Tree': TreeSample,
  'gpui_kit::component::v_virtual_list': VirtualListSample,
};
