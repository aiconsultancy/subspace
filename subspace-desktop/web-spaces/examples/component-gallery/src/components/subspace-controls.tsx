import {
  useId, useRef, useState,
  type ComponentProps, type ComponentType, type CSSProperties,
  type KeyboardEvent, type ReactNode,
} from 'react';
import {
  BookMarked, ChartArea, Check, ChevronDown, Circle, Copy, Eraser,
  ExternalLink, Eye, FileText, FolderOpen, Info, Layers, ListChecks,
  ListFilter, LoaderCircle, Lock, LogIn, MessageSquare, Pencil, Play,
  RotateCcw, RotateCw, Save, Search, Settings, Square, Star, Table,
  TextWrap, Trash, TriangleAlert, X, type LucideIcon,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import './subspace-controls.css';

// Native source: component-gallery/src/samples/{buttons,choices,inputs,
// menus,dialogs,feedback}.rs and their nested builders. All callbacks stay
// inside the fixture: never authenticate, navigate, select a file or copy
// host diagnostics. Native ignore_click actions deliberately remain inert.
function Sample({ caption, children, className = '' }: {
  caption: string; children: ReactNode; className?: string;
}) {
  return <div className={`sample sc-sample ${className}`}>
    <div className="sc-sample-body">{children}</div>
    <div className="sample-caption">{caption}</div>
  </div>;
}

function Samples({ children }: { children: ReactNode }) {
  return <div className="samples sc-samples">{children}</div>;
}

type Intent = 'Accept' | 'Reject' | 'Caution' | 'Primary' | 'Neutral' | 'Quiet';
const intents: { name: Intent; icon: LucideIcon }[] = [
  { name: 'Accept', icon: Check }, { name: 'Reject', icon: X },
  { name: 'Caution', icon: TriangleAlert }, { name: 'Primary', icon: Play },
  { name: 'Neutral', icon: Copy }, { name: 'Quiet', icon: Eye },
];
const variants = {
  Accept: 'success', Reject: 'destructive', Caution: 'warning',
  Primary: 'default', Neutral: 'outline', Quiet: 'ghost',
} as const;

function Action({ icon: Icon, intent = 'Neutral', iconOnly = false,
  children, className = '', ...props }: ComponentProps<typeof Button> & {
  icon?: LucideIcon; intent?: Intent; iconOnly?: boolean;
}) {
  return <Button variant={variants[intent]} className={`sc-action ${iconOnly ? 'sc-icon-only' : ''} ${className}`}
    aria-label={iconOnly && typeof children === 'string' ? children : undefined}
    title={iconOnly && typeof children === 'string' ? children : undefined} {...props}>
    {Icon && <Icon aria-hidden="true" />}{!iconOnly && children}
  </Button>;
}

function SharedIconButton() {
  return <Samples><Sample caption="Small">
    <Action icon={Search} iconOnly intent="Quiet" className="sc-global-icon">Search</Action>
  </Sample></Samples>;
}

function LatchedButton({ initiallyOn }: { initiallyOn: boolean }) {
  const [on, setOn] = useState(initiallyOn);
  return <div className="sc-toolbar-surface"><Action icon={TextWrap} iconOnly intent="Quiet"
    className="sc-global-icon" aria-pressed={on} title="Wrap log lines"
    onClick={() => setOn(!on)}>Wrap</Action></div>;
}

function LatchedButtons() {
  return <Samples>{[false, true].map(on => <Sample key={String(on)} caption={`${on ? 'On' : 'Off'}, Small`}>
    <LatchedButton initiallyOn={on} />
  </Sample>)}</Samples>;
}

function ActionStates() {
  return <Samples>
    <Sample caption="Idle"><Action intent="Primary" icon={Save}>Save</Action></Sample>
    <Sample caption="Busy"><Action intent="Primary" disabled aria-busy="true" icon={LoaderCircle} className="sc-loading">Saving…</Action></Sample>
    <Sample caption="Success"><Action intent="Accept" icon={Check}>Saved</Action></Sample>
    <Sample caption="Failure"><Action intent="Reject" icon={TriangleAlert}>Save failed</Action></Sample>
  </Samples>;
}

function MenuButtons() {
  return <Samples>{['Inactive', 'Active (non-default)', 'Disabled'].map((caption, ix) =>
    <Sample key={caption} caption={caption}><Action className="sc-toolbar-action" icon={ListFilter}
      intent={ix === 1 ? 'Primary' : 'Neutral'} disabled={ix === 2}>
      Severity<ChevronDown aria-hidden="true" />
    </Action></Sample>)}</Samples>;
}

function IntentButtons({ iconOnly = false }: { iconOnly?: boolean }) {
  return <Samples>{intents.map(({ name, icon }) => <Sample key={name} caption={name}>
    <Action intent={name} icon={icon} iconOnly={iconOnly} className={iconOnly ? '' : 'sc-header-action'}>{name}</Action>
  </Sample>)}<Sample caption="Disabled"><Action intent={iconOnly ? 'Quiet' : 'Neutral'}
    icon={iconOnly ? Trash : Square} iconOnly={iconOnly} disabled className={iconOnly ? '' : 'sc-header-action'}>
    {iconOnly ? 'Delete' : 'Stopping…'}
  </Action></Sample></Samples>;
}

function HeaderButtons() { return <IntentButtons />; }
function IconButtons() { return <IntentButtons iconOnly />; }

function ToolbarButtons() {
  return <Samples>{intents.flatMap(({ name, icon }) => [false, true].map(shown =>
    <Sample key={`${name}-${shown}`} caption={`${name}, ${shown ? 'with label' : 'icon only'}`}>
      <Action intent="Quiet" icon={icon} iconOnly={!shown} className={`sc-toolbar-action sc-tone-${name.toLowerCase()}`}>{name}</Action>
    </Sample>))}
    <Sample caption="Disabled"><Action icon={Play} intent="Quiet" className="sc-toolbar-action" disabled>Launch</Action></Sample>
    <Sample caption="Loading"><Action icon={LoaderCircle} intent="Quiet" className="sc-toolbar-action sc-loading" disabled aria-busy="true">Launch</Action></Sample>
  </Samples>;
}

function RecoverySamples({ label, icon, intent, caption = 'Default' }: {
  label: string; icon: LucideIcon; intent: Intent; caption?: string;
}) {
  return <Samples><Sample caption={caption}><Action icon={icon} intent={intent}>{label}</Action></Sample>
    <Sample caption="Disabled"><Action icon={icon} intent={intent} disabled>{label}</Action></Sample></Samples>;
}
function RetryAction() { return <RecoverySamples label="Retry" icon={RotateCw} intent="Primary" />; }
function RecoveryAction() { return <RecoverySamples label="Restart session" icon={RotateCcw} intent="Caution" caption="Caution" />; }
function ConfigureAction() { return <RecoverySamples label="Configure" icon={Settings} intent="Neutral" />; }
function ServiceLink() {
  return <Samples><Sample caption="Default"><Action icon={ExternalLink}>Open in service</Action></Sample></Samples>;
}

function Segments({ options, initial, label }: {
  options: { text: string; icon?: LucideIcon }[]; initial: number | null; label: string;
}) {
  const [selected, setSelected] = useState(initial);
  const group = useRef<HTMLDivElement>(null);
  function move(event: KeyboardEvent<HTMLDivElement>) {
    const index = options.findIndex(option => option.text === (event.target as HTMLElement).getAttribute('aria-label'));
    const next = event.key === 'Home' ? 0 : event.key === 'End' ? options.length - 1
      : event.key === 'ArrowRight' ? (index + 1) % options.length
      : event.key === 'ArrowLeft' ? (index + options.length - 1) % options.length : -1;
    if (next < 0) return;
    event.preventDefault();
    setSelected(next);
    group.current?.querySelectorAll<HTMLButtonElement>('button')[next]?.focus();
  }
  return <div className="sc-segments" role="radiogroup" aria-label={label} ref={group} onKeyDown={move}>
    {options.map(({ text, icon: Icon }, ix) => <Button key={text} variant="ghost"
      role="radio" aria-label={text} aria-checked={selected === ix}
      tabIndex={selected === ix || (selected === null && ix === 0) ? 0 : -1}
      onClick={() => setSelected(ix)}>{Icon && <Icon aria-hidden="true" />}{text}</Button>)}
  </div>;
}

function SegmentedControls() {
  const dates = ['Day', 'Week', 'Month'].map(text => ({ text }));
  return <Samples>
    <Sample caption="No choice yet"><Segments options={dates} initial={null} label="No choice yet" /></Sample>
    <Sample caption="Selected"><Segments options={dates} initial={1} label="Selected period" /></Sample>
    <Sample caption="With icons"><Segments options={[{ text: 'Table', icon: Table }, { text: 'Chart', icon: ChartArea }, { text: 'Layers', icon: Layers }]} initial={0} label="View" /></Sample>
  </Samples>;
}

function TabExample({ mode }: { mode: string }) {
  const [selected, setSelected] = useState(0);
  const id = useId();
  const busy = mode.includes('busy') || mode === 'Busy';
  const count = mode.includes('Count');
  const disabled = mode === 'Disabled';
  const first = mode === 'Plain' ? 'Overview' : 'Issues';
  function navigate(event: KeyboardEvent<HTMLDivElement>) {
    if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return;
    event.preventDefault();
    const next = disabled ? 1 : event.key === 'Home' ? 0 : event.key === 'End' ? 1 : 1 - selected;
    setSelected(next);
    event.currentTarget.querySelectorAll<HTMLButtonElement>('[role="tab"]')[next]?.focus();
  }
  return <div className="sc-tab-example">
    <div role="tablist" aria-label={`${mode} tabs`} className="sc-tab-bar" onKeyDown={navigate}>
      {[first, 'Settings'].map((text, ix) => <Button key={text} role="tab" variant="ghost"
        id={`${id}-tab-${ix}`} aria-controls={`${id}-panel-${ix}`} aria-selected={selected === ix}
        tabIndex={selected === ix || (disabled && ix === 1) ? 0 : -1}
        disabled={disabled && ix === 0} onClick={() => setSelected(ix)}>
        {ix === 1 ? <Settings aria-hidden="true" /> : mode !== 'Plain' && <ListChecks aria-hidden="true" />}
        {text}{ix === 0 && count && <span className="sc-count">12</span>}
        {ix === 0 && busy && <LoaderCircle className="sc-spin" aria-label="Loading" />}
      </Button>)}
    </div>
    {[first, 'Settings'].map((text, ix) => <div key={text} id={`${id}-panel-${ix}`} role="tabpanel"
      aria-labelledby={`${id}-tab-${ix}`} hidden={selected !== ix} className="sc-sr-only">{text}</div>)}
  </div>;
}

function TabBars() {
  return <Samples>{['Icon, selected', 'Plain', 'Count', 'Busy', 'Count and busy', 'Disabled'].map(mode =>
    <Sample caption={mode} key={mode}><TabExample mode={mode} /></Sample>)}</Samples>;
}

function Switch({ label, initial = false, disabled = false, small = false }: {
  label: string; initial?: boolean; disabled?: boolean; small?: boolean;
}) {
  const [checked, setChecked] = useState(initial);
  return <button type="button" className={`sc-switch ${small ? 'sc-switch-small' : ''}`}
    role="switch" aria-label={label} aria-checked={checked} disabled={disabled}
    onClick={() => setChecked(!checked)}><span /></button>;
}

function Switches() {
  return <Samples>{['Window', 'Panel', 'Secondary panel', 'Sidebar', 'Popover', 'Selected row', 'Active list row'].map((surface, ix) =>
    <Sample key={surface} caption={`${surface}: off, on, disabled off, disabled on, small off, small on`}>
      <div className={`sc-switch-surface sc-surface-${ix}`}>
        {['off', 'on', 'disabled off', 'disabled on', 'small off', 'small on'].map((state, stateIx) =>
          <Switch key={state} label={`${surface}, ${state}`} initial={stateIx % 2 === 1} disabled={state.startsWith('disabled')} small={state.startsWith('small')} />)}
      </div>
    </Sample>)}</Samples>;
}

function ToggleRows() {
  const rows = [
    ['On', 'Notifications', true, false], ['Off', 'Background updates', false, false],
    ['Disabled', 'Managed by policy', true, true],
    ['Long label wraps', 'An independent option with a long label that wraps in a narrow panel instead of pushing the switch out', false, false],
  ] as const;
  return <Samples>{rows.map(([caption, label, initial, disabled]) => <Sample caption={caption} key={caption}>
    <div className="sc-toggle-row"><span>{label}</span><Switch label={label} initial={initial} disabled={disabled} /></div>
  </Sample>)}</Samples>;
}

function SliderExample({ disabled = false }: { disabled?: boolean }) {
  const [value, setValue] = useState(40);
  const id = useId();
  return <div className="sc-slider"><div><label htmlFor={id}>{disabled ? 'Locked opacity' : 'Opacity'}</label><output htmlFor={id}>{value}</output></div>
    <input id={id} type="range" min={0} max={100} step={5} value={value} disabled={disabled}
      onChange={event => setValue(Number(event.target.value))} style={{ '--sc-value': `${value}%` } as CSSProperties} />
  </div>;
}
function Sliders() {
  return <Samples><Sample caption="Value"><SliderExample /></Sample><Sample caption="Disabled"><SliderExample disabled /></Sample></Samples>;
}

function LabelledInput({ layout, label, value, error }: { layout?: 'inline'; label: string; value: string; error?: string }) {
  const id = useId();
  return <div className={`sc-field ${layout === 'inline' ? 'sc-field-inline' : ''}`}>
    <label htmlFor={id}>{label}</label><div><Input id={id} defaultValue={value} placeholder="Fixture text"
      aria-invalid={!!error} aria-describedby={error ? `${id}-error` : undefined} />
    {error && <p id={`${id}-error`} className="sc-field-error">{error}</p>}</div>
  </div>;
}
function LabelledInputs() {
  return <Samples>
    <Sample caption="Inline"><LabelledInput layout="inline" label="Inline label" value="Inline value" /></Sample>
    <Sample caption="Stacked"><LabelledInput label="Stacked label" value="Stacked value" /></Sample>
    <Sample caption="Error"><LabelledInput label="Required label" value="" error="Enter a value." /></Sample>
  </Samples>;
}

function CodeEditor() {
  const [value, setValue] = useState('{\n  "board": "component-gallery",\n  "fixture": true\n}');
  return <Samples><Sample caption="Default"><div className="sc-editor">
    <div className="sc-line-numbers" aria-hidden="true">{value.split('\n').map((_, ix) => <div key={ix}>{ix + 1}</div>)}</div>
    <textarea aria-label="JSON sample" spellCheck={false} value={value} onChange={event => setValue(event.target.value)} />
  </div></Sample></Samples>;
}
function FormFields() {
  return <Samples><Sample caption="Label above its control"><LabelledInput label="Name" value="Sample fixture" /></Sample></Samples>;
}
function Headings() {
  return <Samples><Sample caption="Settings page group"><h3 className="sc-heading">Connection</h3></Sample></Samples>;
}
function Notes() {
  return <Samples><Sample caption="Muted help under a control"><p className="sc-control-note">Changes take effect after a restart.</p></Sample></Samples>;
}
function FieldErrors() {
  return <Samples><Sample caption="Validation under its control"><p className="sc-field-error">Enter a port between 1 and 65535.</p></Sample></Samples>;
}

function PathInput({ disabled = false }: { disabled?: boolean }) {
  const [value, setValue] = useState(disabled ? '~/fixtures/locked.json' : '~/fixtures/component-gallery.json');
  return <div className="sc-path-input"><Input aria-label={disabled ? 'Locked fixture file' : 'Fixture file'} disabled={disabled}
    value={value} onChange={event => setValue(event.target.value)} />
    <Action icon={FolderOpen} disabled={disabled} title="Choose a fixture file"
      onClick={() => setValue('~/fixtures/component-gallery.json')}>Choose…</Action>
  </div>;
}
function PathInputs() {
  return <Samples><Sample caption="Choose… opens the file picker"><PathInput /></Sample>
    <Sample caption="Disabled"><PathInput disabled /></Sample></Samples>;
}
function Textareas() {
  return <Samples><Sample caption="Multiline value"><textarea className="sc-control-textarea" rows={3} aria-label="Fixture notes" placeholder="Notes about the fixture" /></Sample></Samples>;
}

function SecretField({ saved = false }: { saved?: boolean }) {
  const [cleared, setCleared] = useState(false);
  const [hinted, setHinted] = useState(false);
  const id = useId();
  const kept = saved && !cleared;
  const label = saved ? 'Token' : 'Password';
  return <div className="sc-secret">
    <div className="sc-secret-heading"><label htmlFor={id}>{label}</label>
      {kept && <span className="sc-saved"><Lock aria-hidden="true" />Token saved</span>}
      <Action icon={cleared ? RotateCcw : Eraser} intent="Quiet" disabled={!saved}
        onClick={() => setCleared(!cleared)}>{cleared ? 'Undo clear' : `Clear saved ${label.toLowerCase()}`}</Action>
    </div>
    <Input id={id} type="password" readOnly={kept} placeholder={saved ? '' : 'Paste a token'}
      aria-label={kept ? 'Token, token saved. Clear the saved token to change it.' : label}
      onClick={() => setHinted(kept)} aria-describedby={kept && hinted ? `${id}-hint` : cleared ? `${id}-clear` : undefined} />
    {kept && hinted && <p className="sc-control-note" id={`${id}-hint`}>Clear the saved token to change it.</p>}
    {cleared && <p className="sc-warning" id={`${id}-clear`}>Save removes the saved token, or replaces it with the token you enter.</p>}
  </div>;
}
function Secrets() {
  return <Samples><Sample caption="Saved; click Clear"><SecretField saved /></Sample>
    <Sample caption="Not saved"><SecretField /></Sample></Samples>;
}
function RevealButtons() {
  const platform = typeof navigator === 'undefined' ? '' : navigator.platform;
  const label = /Mac/i.test(platform) ? 'Show in Finder' : /Win/i.test(platform) ? 'Show in File Explorer' : 'Show in Folder';
  return <Samples><Sample caption="Platform label; disabled so the gallery opens no folder"><Action icon={FolderOpen} disabled>{label}</Action></Sample></Samples>;
}

type PopupProps = {
  label: string; icon?: LucideIcon; children: (close: () => void) => ReactNode;
  triggerContent?: ReactNode;
  menu?: boolean; active?: boolean; context?: boolean; className?: string;
};

// greybeard: the browser Popover API owns top-layer display, light dismissal,
// Escape and focus restoration; bounded fixture menus need no new dependency.
function Popup({ label, icon: Icon, children, triggerContent, menu = false, active = false, context = false, className = '' }: PopupProps) {
  const id = useId();
  const trigger = useRef<HTMLButtonElement>(null);
  const popup = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);
  function close() { popup.current?.hidePopover(); trigger.current?.focus(); }
  function focusPopup() {
    popup.current?.querySelector<HTMLElement>('input, button:not(:disabled), [tabindex="0"]')?.focus();
  }
  function openPopup() {
    popup.current?.showPopover();
    setOpen(true);
    // Keyboard opening transfers focus before the next key arrives. Popover's
    // toggle event is queued; a React effect leaves focus on the trigger.
    focusPopup();
  }
  function position() {
    const box = trigger.current?.getBoundingClientRect();
    const panel = popup.current;
    if (!box || !panel) return;
    panel.style.left = `${Math.max(8, Math.min(box.left, window.innerWidth - 340))}px`;
    panel.style.top = `${Math.min(box.bottom + 4, Math.max(8, window.innerHeight - 280))}px`;
    panel.style.maxHeight = `${Math.max(100, window.innerHeight - Number.parseFloat(panel.style.top) - 8)}px`;
  }
  function keys(event: KeyboardEvent<HTMLDivElement>) {
    if (!menu) return;
    const items = [...event.currentTarget.querySelectorAll<HTMLButtonElement>('[role^="menuitem"]:not(:disabled)')];
    const ix = items.indexOf(document.activeElement as HTMLButtonElement);
    const next = event.key === 'Home' ? 0 : event.key === 'End' ? items.length - 1
      : event.key === 'ArrowDown' ? (ix + 1) % items.length
      : event.key === 'ArrowUp' ? (ix - 1 + items.length) % items.length : -1;
    if (next >= 0 && !(event.target instanceof HTMLInputElement && ['Home', 'End'].includes(event.key))) {
      event.preventDefault(); items[next]?.focus();
    } else if (event.key.length === 1 && !(event.target instanceof HTMLInputElement)) {
      const match = [...items.slice(ix + 1), ...items.slice(0, ix + 1)].find(item => item.textContent?.trim().toLowerCase().startsWith(event.key.toLowerCase()));
      if (match) { event.preventDefault(); match.focus(); }
    } else if (event.key === 'Tab') popup.current?.hidePopover();
  }
  return <div className="sc-popup-owner">
    <Action ref={trigger} icon={Icon} intent={active ? 'Primary' : 'Neutral'} className={context ? 'sc-context-row' : ''}
      popoverTarget={id} aria-label={label} aria-haspopup={menu ? 'menu' : 'dialog'} aria-expanded={open} aria-controls={id}
      onKeyDown={event => {
        if (event.key === 'ArrowDown' && menu) { event.preventDefault(); openPopup(); }
      }} onContextMenu={context ? event => { event.preventDefault(); openPopup(); } : undefined}>
      {triggerContent ?? label}{!context && menu && <ChevronDown aria-hidden="true" />}
    </Action>
    <div id={id} ref={popup} popover="auto" role={menu ? 'menu' : 'dialog'} aria-label={label}
      className={`sc-control-popup ${className}`} onBeforeToggle={position}
      onToggle={event => {
        const opening = event.newState === 'open';
        setOpen(opening);
        // Pointer opening uses the native popover target. Never reset focus
        // after keyboard navigation has already moved within the menu.
        if (opening && !popup.current?.contains(document.activeElement)) focusPopup();
      }} onKeyDown={keys} tabIndex={-1}>
      {children(close)}
    </div>
  </div>;
}

function MenuItem({ children, checked, icon: Icon, ...props }: ComponentProps<'button'> & { checked?: boolean; icon?: LucideIcon }) {
  return <button type="button" className="sc-menu-item" role={checked === undefined ? 'menuitem' : 'menuitemcheckbox'}
    aria-checked={checked} tabIndex={-1} {...props}>
    <span className="sc-menu-slot">{checked ? <Check aria-hidden="true" /> : Icon && <Icon aria-hidden="true" />}</span>{children}
  </button>;
}
function DropdownMenu() {
  const [checked, setChecked] = useState(true);
  return <Samples><Sample caption="Closed; click to open"><Popup label="Options" icon={ListFilter} menu>{close => <>
    <MenuItem checked={checked} onClick={() => { setChecked(!checked); close(); }}>Checked option</MenuItem>
    <MenuItem onClick={close}>Plain option</MenuItem><hr />
    <MenuItem disabled>Disabled option</MenuItem>
  </>}</Popup></Sample></Samples>;
}

function Selects() {
  const options = ['Release', 'Debug', 'Profile'];
  return <Samples>{['Value', 'Placeholder', 'Small', 'Disabled'].map((caption, ix) => <Sample caption={caption} key={caption}>
    <select className={`sc-select ${ix === 2 ? 'sc-select-small' : ''}`} aria-label={['Build profile', 'Unchosen build profile', 'Small build profile', 'Locked build profile'][ix]}
      defaultValue={ix === 1 ? '' : ix === 2 ? 'Debug' : 'Release'} disabled={ix === 3}>
      {ix === 1 && <option value="" disabled>Choose a profile</option>}{options.map(option => <option key={option}>{option}</option>)}
    </select>
  </Sample>)}</Samples>;
}

function ColorSelects() {
  const [color, setColor] = useState('#0969da');
  const [hexText, setHexText] = useState('#0969da');
  const [hsla, setHsla] = useState([212, 92, 45, 100]);
  const palette = ['#0969da', '#cf222e', '#1a7f37', '#9a6700', '#8250df', '#bf3989', '#0550ae', '#57606a'];
  function hexChanged(hex: string) {
    setColor(hex);
    setHexText(hex);
    const [r, g, b] = [1, 3, 5].map(offset => Number.parseInt(hex.slice(offset, offset + 2), 16) / 255);
    const max = Math.max(r, g, b), min = Math.min(r, g, b), delta = max - min;
    const l = (max + min) / 2;
    const h = delta === 0 ? 0 : max === r ? ((g - b) / delta + 6) % 6 : max === g ? (b - r) / delta + 2 : (r - g) / delta + 4;
    setHsla([Math.round(h * 60), Math.round(delta === 0 ? 0 : delta / (1 - Math.abs(2 * l - 1)) * 100), Math.round(l * 100), 100]);
  }
  function channelChanged(ix: number, value: number) {
    const next = hsla.map((channel, channelIx) => ix === channelIx ? value : channel);
    setHsla(next);
    const [h, s, l] = [next[0], next[1] / 100, next[2] / 100];
    const a = s * Math.min(l, 1 - l);
    const channel = (n: number) => { const k = (n + h / 30) % 12; return Math.round(255 * (l - a * Math.max(-1, Math.min(k - 3, 9 - k, 1)))).toString(16).padStart(2, '0'); };
    const hex = `#${channel(0)}${channel(8)}${channel(4)}`;
    setColor(hex);
    setHexText(hex);
  }
  return <Samples><Sample caption="Swatch and hex value; click for the palette, HSLA and hex">
    <Popup label="Fixture colour" className="sc-color-popup" triggerContent={<span className="sc-colour-value"><span className="sc-colour-swatch" style={{ background: color, opacity: hsla[3] / 100 }} />{color}</span>}>{() => <>
      <div className="sc-palette">{palette.map(hex => <button key={hex} type="button" aria-label={hex}
        aria-pressed={color === hex} style={{ background: hex }} onClick={() => hexChanged(hex)} />)}</div>
      <label className="sc-colour-hex">Hex<Input aria-label="Hex colour" value={hexText}
        aria-invalid={!/^#[0-9a-f]{6}$/i.test(hexText)} onChange={event => {
          const hex = event.target.value;
          setHexText(hex);
          if (/^#[0-9a-f]{6}$/i.test(hex)) hexChanged(hex);
        }} onBlur={() => setHexText(color)} /></label>
      <div className="sc-channels">{['H', 'S', 'L', 'A'].map((label, ix) => <label key={label}>{label}
        <Input type="number" aria-label={['Hue', 'Saturation', 'Lightness', 'Alpha'][ix]} min={0} max={ix === 0 ? 360 : 100}
          value={hsla[ix]} onChange={event => channelChanged(ix, Math.max(0, Math.min(ix === 0 ? 360 : 100, Number(event.target.value))))} />
      </label>)}</div>
    </>}</Popup>
  </Sample></Samples>;
}

function FilterMenu({ close }: { close: () => void }) {
  const [query, setQuery] = useState('');
  const [checked, setChecked] = useState(true);
  return <><h4>Gallery sample menu</h4><Input aria-label="Filter the sample options" placeholder="Filter the sample options…"
    value={query} onChange={event => setQuery(event.target.value)} />
    {['Checked option', 'Option with a glyph', 'Plain option'].filter(option => option.toLowerCase().includes(query.toLowerCase())).map(option =>
      <MenuItem key={option} checked={option === 'Checked option' ? checked : undefined} icon={option === 'Option with a glyph' ? Circle : undefined}
        onClick={() => { if (option === 'Checked option') setChecked(!checked); close(); }}>{option}</MenuItem>)}
    {!['Checked option', 'Option with a glyph', 'Plain option'].some(option => option.toLowerCase().includes(query.toLowerCase())) && <p className="sc-control-note">No matching options</p>}
  </>;
}
function MenuAnchors() {
  return <Samples><Sample caption="Closed; click to open"><Popup label="Sample menu" icon={ListFilter} menu>
    {close => <FilterMenu close={close} />}</Popup></Sample></Samples>;
}
function Anchors() {
  return <Samples><Sample caption="Retained popover body; Escape closes it"><Popup label="Show popover" icon={MessageSquare}>
    {() => <div className="sc-popover-body" tabIndex={0}><strong>Fixture popover</strong>
      <p>Contextual content that does not interrupt the task. Press Escape to close.</p></div>}
  </Popup></Sample></Samples>;
}
function RowMenus() {
  return <Samples><Sample caption="A row's standard context menu"><Popup label="#42 Sample fixture · right-click" icon={MessageSquare} menu context>
    {close => <><MenuItem onClick={close}>Add to chat</MenuItem><hr />
      <MenuItem onClick={close}>Open details</MenuItem><MenuItem onClick={close}>Open in GitHub</MenuItem><hr />
      {['Copy link', 'Copy title', 'Copy id'].map(label => <MenuItem key={label} onClick={close}>{label}</MenuItem>)}</>}
  </Popup></Sample></Samples>;
}

const repositories = ['example/component-gallery', 'example/fixtures', 'example/sample-app', 'example/docs', 'example/tools'];
function RepositoryPicker() {
  const [favourites, setFavourites] = useState<string[]>([]);
  const [filter, setFilter] = useState(false);
  const [query, setQuery] = useState('');
  const matches = repositories.filter(repo => repo.includes(query.toLowerCase()));
  function toggle(repo: string) {
    setFavourites(current => current.includes(repo) ? current.filter(item => item !== repo) : [...current, repo]);
  }
  return <Samples><Sample caption="Fixture search; star to favourite">
    <Popup label={`Repositories (${favourites.length} favourite)`} icon={BookMarked} menu active={favourites.length > 0}>
      {() => <><h4>Repositories</h4><Input aria-label="Search repositories on GitHub" placeholder="Search repositories on GitHub"
        value={query} onChange={event => setQuery(event.target.value)} />
        <div className="sc-menu-heading">Mine, Review requested, Involved</div>
        <MenuItem checked={filter} onClick={() => setFilter(!filter)}>Favourites only</MenuItem>
        {[true, false].map(starred => <div role="group" aria-label={starred ? 'Favourites' : query ? 'GitHub results' : 'Your repositories'} key={String(starred)}>
          {matches.some(repo => favourites.includes(repo) === starred) && <div className="sc-menu-heading">{starred ? 'Favourites' : query ? 'GitHub results' : 'Your repositories'}</div>}
          {matches.filter(repo => favourites.includes(repo) === starred).map(repo => <MenuItem key={repo} onClick={() => toggle(repo)}
            aria-label={`${repo}, ${starred ? 'favourite' : 'not a favourite'}`} icon={Star}>
            <span className="sc-repo-name">{repo}</span><Star className={starred ? 'sc-starred' : ''} aria-hidden="true" />
          </MenuItem>)}
        </div>)}
        {matches.length === 0 && <p className="sc-control-note">No repositories found</p>}
      </>}
    </Popup>
  </Sample></Samples>;
}

function DialogFooter({ kind = 'save' }: { kind?: 'save' | 'delete' | 'discard' }) {
  const discard = kind === 'discard';
  return <div className="sc-dialog-footer">
    <Action icon={discard ? Pencil : X} className="sc-dialog-action">{discard ? 'Keep editing' : 'Cancel'}</Action>
    <Action icon={kind === 'save' ? Check : discard ? Eraser : Trash} intent={kind === 'save' ? 'Primary' : 'Reject'} className="sc-dialog-action">
      {kind === 'save' ? 'Save' : discard ? 'Discard' : 'Delete'}
    </Action>
  </div>;
}
function DialogFrame({ title }: { title: string }) {
  const id = useId();
  // The native gallery draws these frames inline, not as modal windows.
  return <section role="dialog" aria-labelledby={id} className="sc-dialog-frame">
    <h3 id={id}>{title}</h3><p>Fixture body text of a dashboard dialog.</p><DialogFooter />
  </section>;
}
function DialogFrames() {
  return <Samples><Sample caption="D540 with footer"><DialogFrame title="Save the changes?" /></Sample></Samples>;
}
function DialogFooters() {
  return <Samples>{(['save', 'delete', 'discard'] as const).map(kind => <Sample key={kind}
    caption={kind === 'save' ? 'Cancel → Save' : kind === 'delete' ? 'Cancel → Delete' : 'Keep editing → Discard'}>
    <div className="sc-footer-example"><DialogFooter kind={kind} /></div>
  </Sample>)}</Samples>;
}
function DialogBackdrop() {
  return <Samples><Sample caption="Bounded"><div className="sc-backdrop-example"><div className="sc-backdrop">
    <DialogFrame title="Dialog over a backdrop" />
  </div></div></Sample></Samples>;
}

function CopyDetails() {
  const [notice, setNotice] = useState('');
  return <Samples>
    <Sample caption="Error, activate to copy and confirm"><div className="sc-action-row">
      <Action icon={Copy} onClick={() => setNotice('Details copied (fixture only)')}>Copy details</Action>
      <Action icon={FileText} onClick={() => setNotice('Fixture request: The fixture request returned 503 Service Unavailable.')}>Show log</Action>
    </div></Sample>
    <Sample caption="About, without an error"><Action icon={Copy} onClick={() => setNotice('Diagnostics copied (fixture only)')}>Copy diagnostics</Action></Sample>
    {notice && <output className="sc-local-notice" aria-live="polite">{notice}</output>}
  </Samples>;
}

function SignInPhase({ phase }: { phase: 'starting' | 'browser' | 'device' }) {
  const [cancelled, setCancelled] = useState(false);
  const [notice, setNotice] = useState('');
  const message = phase === 'starting' ? 'Starting sign-in…' : phase === 'browser'
    ? 'Continue in your browser. Subspace finishes signing in when GitHub sends you back.'
    : 'Enter this code on GitHub. Subspace finishes signing in when GitHub confirms it.';
  if (cancelled) return <p role="status" className="sc-control-note">Sign-in cancelled</p>;
  return <div className="sc-sign-in-phase"><p className="sc-control-note">{message}</p>
    {phase === 'device' && <div className="sc-device-code">OCTO-1234</div>}
    <div className="sc-action-row">{phase === 'device' && <>
      <Action icon={Copy} onClick={() => setNotice('Code copied (fixture only)')}>Copy code</Action>
      <Action icon={ExternalLink} onClick={() => setNotice('Fixture only: no GitHub sign-in starts.')}>Open GitHub</Action>
    </>}<Action icon={X} intent="Quiet" onClick={() => setCancelled(true)}>Cancel</Action></div>
    <span role="status" className="sc-control-note">{notice}</span>
  </div>;
}
function GithubSignIn() {
  return <Samples>
    <Sample caption="Sign in with GitHub, ready"><Action icon={LogIn}>Sign in with GitHub</Action></Sample>
    <Sample caption="Sign in with GitHub, while a sign-in runs"><Action icon={LogIn} disabled>Sign in with GitHub</Action></Sample>
    <Sample caption="Starting"><SignInPhase phase="starting" /></Sample>
    <Sample caption="Continue in the browser"><SignInPhase phase="browser" /></Sample>
    <Sample caption="Device code, with Copy code, Open GitHub and Cancel"><SignInPhase phase="device" /></Sample>
    <Sample caption="Failed"><p className="sc-field-error">GitHub sign-in was declined in the browser.</p></Sample>
  </Samples>;
}

function Skeleton() {
  return <Samples><Sample caption="Loading"><div className="sc-skeleton" role="status" aria-label="Loading fixture rows">
    {[220, 300, 180, 260, 240, 150, 280, 200].map((width, ix) => <div className={`sc-skeleton-row ${ix % 4 === 0 ? 'sc-skeleton-parent' : ''}`} key={ix} aria-hidden="true">
      <span className="sc-skeleton-id" /><span style={{ width }} />
    </div>)}
  </div></Sample></Samples>;
}
function LoadingLine() {
  return <Samples><Sample caption="Loading"><p className="sc-control-note" role="status">Loading fixture rows…</p></Sample></Samples>;
}
function EmptyLine() {
  return <Samples><Sample caption="Empty"><p className="sc-control-note">No failed fixture rows</p></Sample></Samples>;
}

const serviceError = 'The fixture service answered 503 Service Unavailable.';
type FeedbackKind = 'empty' | 'configuration' | 'settings' | 'actions' | 'banner' | 'banner-actions';
function Feedback({ kind }: { kind: FeedbackKind }) {
  const [dismissed, setDismissed] = useState(false);
  const banner = kind.startsWith('banner');
  const error = !['empty', 'configuration'].includes(kind);
  const title = kind === 'empty' ? 'Nothing to show' : kind === 'configuration' ? 'Not configured'
    : kind === 'settings' ? 'Could not sign in' : kind === 'banner' ? 'Some rows are missing'
    : kind === 'banner-actions' ? 'Some runs are missing' : 'Could not load';
  const message = kind === 'empty' ? 'No fixture rows match the filter.'
    : kind === 'configuration' ? 'Add a fixture service in Settings.' : serviceError;
  const caption = kind === 'empty' ? 'Empty' : kind === 'configuration' ? 'Needs configuration'
    : kind === 'settings' ? 'Error with Retry and Configure' : kind === 'banner' ? 'Error with Retry' : 'Error with actions';
  return <Samples><Sample caption={caption}><div className={`sc-feedback-bound ${banner ? 'sc-feedback-line' : ''}`}>
    {!dismissed && <Alert role={error ? 'alert' : 'status'} variant={error ? 'destructive' : 'default'} className={banner ? 'sc-error-banner' : 'sc-feedback-state'}>
      <AlertTitle>{title}</AlertTitle><AlertDescription>{message}</AlertDescription>
      {kind !== 'empty' && <div className="sc-action-row">
        {error && kind !== 'banner-actions' && <Action icon={RotateCw} intent="Primary">Retry</Action>}
        {['configuration', 'settings'].includes(kind) && <Action icon={Settings}>Configure</Action>}
        {kind === 'actions' && <Action icon={ExternalLink}>Open logs</Action>}
        {kind === 'banner-actions' && <Action icon={X} intent="Quiet" onClick={() => setDismissed(true)}>Dismiss</Action>}
      </div>}
    </Alert>}
  </div></Sample></Samples>;
}
function Empty() { return <Feedback kind="empty" />; }
function Configuration() { return <Feedback kind="configuration" />; }
function ErrorWithSettings() { return <Feedback kind="settings" />; }
function ErrorWithActions() { return <Feedback kind="actions" />; }
function ErrorBanner() { return <Feedback kind="banner" />; }
function ErrorBannerWithActions() { return <Feedback kind="banner-actions" />; }

function Toasts() {
  const [toast, setToast] = useState<'info' | 'error' | null>(null);
  return <Samples>
    <Sample caption="Info"><Action icon={Info} onClick={() => setToast('info')}>Show info toast</Action></Sample>
    <Sample caption="Error"><Action icon={TriangleAlert} onClick={() => setToast('error')}>Show error toast</Action></Sample>
    {toast && <div className={`sc-toast sc-toast-${toast}`} role={toast === 'error' ? 'alert' : 'status'}>
      {toast === 'error' ? <TriangleAlert aria-hidden="true" /> : <Info aria-hidden="true" />}
      <span>Sample {toast === 'info' ? 'information' : 'error'} toast from the Component gallery.</span>
      <Action icon={X} iconOnly intent="Quiet" onClick={() => setToast(null)}>Dismiss notification</Action>
    </div>}
  </Samples>;
}

export const subspaceControlSamples: Record<string, ComponentType> = {
  'icon_button::IconButton': SharedIconButton,
  'icon_button::latched': LatchedButtons,
  'action::ActionState': ActionStates,
  'action::menu_button': MenuButtons,
  'action::header_button': HeaderButtons,
  'action::icon_button': IconButtons,
  'action::toolbar_button': ToolbarButtons,
  'feedback::retry_action': RetryAction,
  'feedback::recovery_action': RecoveryAction,
  'feedback::configure_action': ConfigureAction,
  'panel::service_link': ServiceLink,
  'segmented::control': SegmentedControls,
  'tabs::tab_bar': TabBars,
  'toggle::switch': Switches,
  'toggle::toggle_row': ToggleRows,
  'slider::control': Sliders,
  'fields::labelled_input': LabelledInputs,
  'fields::code_editor': CodeEditor,
  'fields::form_field': FormFields,
  'fields::heading': Headings,
  'fields::note': Notes,
  'fields::field_error': FieldErrors,
  'fields::path_input': PathInputs,
  'fields::textarea': Textareas,
  'secret::secret_field': Secrets,
  'reveal::button': RevealButtons,
  'anchored::dropdown_menu': DropdownMenu,
  'anchored::select': Selects,
  'anchored::color_select': ColorSelects,
  'menu::MenuAnchors': MenuAnchors,
  'anchored::Anchor': Anchors,
  'row_menu::RowMenu': RowMenus,
  'repositories::RepositoryPicker': RepositoryPicker,
  'dialog::frame': DialogFrames,
  'dialog::footer': DialogFooters,
  'dialog::backdrop': DialogBackdrop,
  'copy_details::CopyDetails': CopyDetails,
  'github_sign_in::status': GithubSignIn,
  'feedback::skeleton': Skeleton,
  'feedback::loading_line': LoadingLine,
  'feedback::empty_line': EmptyLine,
  'feedback::empty': Empty,
  'feedback::configuration': Configuration,
  'feedback::error_box_with_settings': ErrorWithSettings,
  'feedback::error_box_with_actions': ErrorWithActions,
  'feedback::error_banner': ErrorBanner,
  'feedback::error_banner_with_actions': ErrorBannerWithActions,
  Toast: Toasts,
};
