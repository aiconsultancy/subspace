import { useId, useRef, useState, type ComponentType, type CSSProperties, type KeyboardEvent, type ReactNode } from 'react';
import { ArrowDown, ArrowUp, ArrowUpDown, Check, ChevronDown, ChevronRight, Copy, Download, ExternalLink, Filter, GitPullRequest, Hash, MessageSquare, RefreshCw, ScrollText, Share2, Trash2, User, X } from 'lucide-react';
import { cards } from '@/catalogue';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Alert, AlertDescription } from '@/components/ui/alert';
import './subspace-content.css';

// Fixtures follow samples/{content,messaging,navigation,lists,inconsistencies,shell}.rs.
// Local callbacks expose intent only; no native command, account or service runs.
const fixtureUrl = 'https://example.com/component-gallery';
const longTitle = 'A long fixture title that does not fit in the narrow sample box and is shortened';
const markdownFixture = '**Bold**, *italic*, `code` and a [link](https://example.com/component-gallery).\n\n- First point\n- Second point';

function Sample({ caption, children, className = '' }: { caption: string; children: ReactNode; className?: string }) {
  return <div className={`sample sc-sample ${className}`}><div>{children}</div><div className="sample-caption">{caption}</div></div>;
}

function External({ children = 'Example link', marked = false }: { children?: ReactNode; marked?: boolean }) {
  return <a className="sc-link" href={fixtureUrl} target="_blank" rel="noopener noreferrer">{children}{marked && <ExternalLink aria-hidden="true" />}</a>;
}

function BadgeSamples() {
  return <div className="samples sc-content">
    <Sample caption="Count"><span className="sc-badge">12</span></Sample>
    <Sample caption="Accent"><span className="sc-badge sc-badge-accent">New</span></Sample>
    <Sample caption="Key hint"><span className="sc-badge sc-muted">3</span></Sample>
  </div>;
}

function ExternalSample() {
  return <div className="samples sc-content"><Sample caption="Default"><External /></Sample></div>;
}

function MarkedExternalSample() {
  return <div className="samples sc-content"><Sample caption="Marked"><External marked /></Sample></div>;
}

function TruncatedSamples() {
  return <div className="samples sc-content">
    <Sample caption="Shortened"><span className="sc-truncated" title={longTitle} tabIndex={0}>{longTitle}</span></Sample>
    <Sample caption="Wrapped"><span className="sc-wrap-title">{longTitle}</span></Sample>
  </div>;
}

function PlainTextSample() {
  const [status, setStatus] = useState('');
  const value = useRef<HTMLSpanElement>(null);
  async function copyText() {
    const selection = window.getSelection();
    const text = value.current?.contains(selection?.anchorNode ?? null) && value.current?.contains(selection?.focusNode ?? null) && selection?.toString().trim() ? selection.toString() : 'Select this fixture text.';
    try { await navigator.clipboard.writeText(text); setStatus('Copied fixture text'); }
    catch { setStatus('Clipboard unavailable. Select the text and copy it.'); }
  }
  return <div className="samples sc-content"><Sample caption="Selectable">
    <div className="sc-selectable"><span ref={value}>Select this fixture text.</span>
      <details><summary>Text actions</summary><Button variant="ghost" onClick={copyText}><Copy aria-hidden="true" />Copy text</Button></details>
      <span role="status" className="sc-note">{status}</span>
    </div>
  </Sample></div>;
}

// greybeard: bounded Markdown preview, not a second rich-text engine. It covers
// the native fixture's bold/italic/code/HTTPS links and bullet paragraphs.
// Unknown syntax remains text. A general editor needs an approved Markdown lib.
function inlineMarkdown(text: string): ReactNode[] {
  return text.split(/(\*\*[^*]+\*\*|\*[^*]+\*|`[^`]+`|\[[^\]]+\]\(https?:\/\/[^\s)]+\))/g).map((part, ix) => {
    if (part.startsWith('**') && part.endsWith('**')) return <strong key={ix}>{part.slice(2, -2)}</strong>;
    if (part.startsWith('*') && part.endsWith('*')) return <em key={ix}>{part.slice(1, -1)}</em>;
    if (part.startsWith('`') && part.endsWith('`')) return <code key={ix}>{part.slice(1, -1)}</code>;
    const link = /^\[([^\]]+)\]\((https?:\/\/[^\s)]+)\)$/.exec(part);
    if (link) return <a key={ix} href={link[2]} target="_blank" rel="noopener noreferrer">{link[1]}</a>;
    return part;
  });
}

function Markdown({ text }: { text: string }) {
  if (!text.trim()) return <p className="sc-muted">Nothing to preview.</p>;
  return <div className="sc-rich">{text.split(/\n\s*\n/).map((block, ix) => block.split('\n').every(line => line.startsWith('- '))
    ? <ul key={ix}>{block.split('\n').map((line, lineIx) => <li key={lineIx}>{inlineMarkdown(line.slice(2))}</li>)}</ul>
    : <p key={ix}>{inlineMarkdown(block)}</p>)}</div>;
}

function RichTextSamples() {
  return <div className="samples sc-content">
    <Sample caption="Markdown"><div className="sc-rich-width"><Markdown text={markdownFixture} /></div></Sample>
    <Sample caption="HTML"><div className="sc-rich sc-rich-width"><p><b>Bold</b> HTML with <code>code</code>.</p></div></Sample>
  </div>;
}

function GroupHeadingSample() {
  return <div className="samples sc-content"><Sample caption="Default"><h3 className="sc-group-heading">Group heading</h3></Sample></div>;
}

function ToolbarLabelSample() {
  return <div className="samples sc-content"><Sample caption="One account"><span className="sc-toolbar-label" aria-label="Signed in to GitHub as octo-dev"><User aria-hidden="true" />octo-dev</span></Sample></div>;
}

function Avatar({ name, kind = 'person' }: { name: string; kind?: 'person' | 'sender' | 'member' }) {
  const initials = name.split(' ').map(word => word[0]).slice(0, 2).join('');
  return <span className={`sc-avatar sc-avatar-${kind}`} role="img" aria-label={name} title={name}>{initials}</span>;
}

function PersonSample() {
  return <div className="samples sc-content"><Sample caption="Initials"><Avatar name="Ada Lovelace" /></Sample></div>;
}

function GroupSamples() {
  return <div className="samples sc-content">{[
    { caption: 'One person', names: ['Ada Lovelace'] },
    { caption: 'Two people', names: ['Ada Lovelace', 'Grace Hopper'] },
    { caption: 'Three people (limit 2)', names: ['Ada Lovelace', 'Grace Hopper', 'Alan Turing'] },
  ].map(({ caption, names }) => <Sample key={caption} caption={caption}><div className="sc-avatar-group">{names.slice(0, 2).map(name => <Avatar key={name} name={name} />)}</div></Sample>)}</div>;
}

function ChatEditor({ message = false }: { message?: boolean }) {
  const [text, setText] = useState('');
  const [status, setStatus] = useState('');
  function submit() { if (text.trim()) setStatus('Submit event emitted locally. No message was sent.'); }
  function onKeyDown(event: KeyboardEvent<HTMLTextAreaElement>) {
    if (event.key !== 'Enter' || event.nativeEvent.isComposing) return;
    if (event.altKey) {
      event.preventDefault();
      const { selectionStart, selectionEnd } = event.currentTarget;
      setText(text.slice(0, selectionStart) + '\n' + text.slice(selectionEnd));
    } else if (!event.shiftKey) { event.preventDefault(); submit(); }
  }
  return <div className="sc-chat-editor">
    <textarea className="sc-textarea" aria-label={message ? 'Message #fixtures' : 'Ask Copilot'}
      placeholder={message ? 'Message #fixtures' : 'Ask Copilot, / for commands'} rows={2}
      value={text} onChange={event => { setText(event.target.value); setStatus(''); }} onKeyDown={onKeyDown} />
    {message && <div className="sc-editor-footer"><span className="sc-note">Enter to send · Shift+Enter for a new line</span>
      <Button aria-label="Send message" disabled={!text.trim()} onClick={submit}><ArrowUp aria-hidden="true" /></Button></div>}
    <span className="sc-note" role="status">{status}</span>
  </div>;
}

function ChatInputSamples() {
  return <div className="samples sc-content">
    <Sample caption="Agent: Markdown input"><div className="sc-chat-sample"><div className="sc-chat-bound"><ChatEditor /></div><p className="sc-note">The native agent fixture has no toolbar. Write/Preview is shown in Composer.</p></div></Sample>
    <Sample caption="Message: plain text, Return sends"><div className="sc-chat-bound"><ChatEditor message /></div></Sample>
  </div>;
}

const completions = ['/join', 'Ada Example', 'Sam Fixture', '#fixtures'];

function CompletionSample() {
  const [selected, setSelected] = useState(0);
  const [status, setStatus] = useState('');
  const refs = useRef<(HTMLButtonElement | null)[]>([]);
  function onKeyDown(event: KeyboardEvent<HTMLButtonElement>, ix: number) {
    const next = event.key === 'ArrowDown' ? (ix + 1) % 4 : event.key === 'ArrowUp' ? (ix + 3) % 4 : event.key === 'Home' ? 0 : event.key === 'End' ? 3 : null;
    if (next !== null) { event.preventDefault(); setSelected(next); refs.current[next]?.focus(); }
  }
  return <div className="samples sc-content"><Sample caption="Command, people and a channel"><div className="sc-completions">
    <div role="listbox" aria-label="Completion choices">{completions.map((name, ix) => <button type="button" role="option" aria-selected={selected === ix} tabIndex={selected === ix ? 0 : -1}
      key={name} ref={node => { refs.current[ix] = node; }} onKeyDown={event => onKeyDown(event, ix)}
      className={`sc-completion ${ix === 2 ? 'sc-away' : ''}`} onClick={() => { setSelected(ix); setStatus(`Accepted ${name} locally; no command runs.`); }}>
      {(ix === 1 || ix === 2) && <Avatar name={name} kind="member" />}<span>{name}</span>
      {ix === 0 && <><span className="sc-muted">#channel</span><span className="sc-completion-detail">Join a channel</span></>}
      {ix === 1 && <span className="sc-completion-detail">@</span>}
    </button>)}</div><span className="sc-note" role="status">{status}</span>
  </div></Sample></div>;
}

function ComposerSample() {
  const [text, setText] = useState('');
  const [preview, setPreview] = useState(false);
  const [asked, setAsked] = useState(false);
  const [status, setStatus] = useState('');
  const editor = useRef<HTMLTextAreaElement>(null);
  const id = useId();
  function cancel() {
    if (text.trim() && !asked) { setAsked(true); return; }
    setAsked(false); setStatus('Cancel event emitted locally. The gallery keeps its fixture editor.');
  }
  function submit() { if (text.trim()) setStatus('Submit event emitted locally. No comment was posted.'); }
  function onKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    if (event.nativeEvent.isComposing) return;
    if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); cancel(); }
    if (event.key === 'Enter' && (event.ctrlKey || event.metaKey)) { event.preventDefault(); submit(); }
  }
  return <div className="samples sc-content"><Sample caption="Write and Preview with a submit action"><div className="sc-composer" onKeyDown={onKeyDown}>
    <div className="sc-composer-header"><label htmlFor={id}>Comment</label>
      <div className="sc-mode-buttons" role="group" aria-label="Comment mode">{['Write', 'Preview'].map((label, ix) => <Button key={label}
        variant={preview === Boolean(ix) ? 'selected' : 'ghost'} aria-pressed={preview === Boolean(ix)}
        onClick={() => { setPreview(Boolean(ix)); if (!ix) requestAnimationFrame(() => editor.current?.focus()); }}>{label}</Button>)}</div><span className="sc-note">Markdown</span></div>
    {preview ? <div className="sc-preview" tabIndex={0} aria-label="Comment preview"><Markdown text={text} /></div>
      : <textarea id={id} ref={editor} className="sc-textarea" rows={4} placeholder="Leave a fixture comment" value={text}
        onChange={event => { setText(event.target.value); setAsked(false); setStatus(''); }} />}
    <div className="sc-editor-footer"><span className={`sc-note ${asked ? 'sc-warning' : ''}`}>{asked ? 'Press Esc again to discard the changes.' : 'Ctrl+Enter: submit · Esc to cancel'}</span>
      <Button variant="outline" onClick={cancel}><X aria-hidden="true" />Cancel</Button><Button disabled={!text.trim()} onClick={submit}><Check aria-hidden="true" />Submit</Button></div>
    <span role="status" className="sc-note">{status}</span>
    <span className="sc-note">Preview supports this fixture’s bold, italic, code, links and bullet lists.</span>
  </div></Sample></div>;
}

function BubbleSample() {
  return <div className="samples sc-content"><Sample caption="Transcript bubble"><div className="sc-bubble-bound"><div className="sc-bubble">A fixture reply inside the shared bubble.</div></div></Sample></div>;
}

function MessageSample() {
  return <div className="samples sc-content"><Sample caption="Sender, header and text"><div className="sc-messages">{[
    ['Ada Example', 'Is the fixture ready?'], ['Sam Fixture', 'Yes, open the gallery.'],
  ].map(([name, text]) => <div className="sc-message" key={name}><Avatar kind="sender" name={name} /><div><strong>{name}</strong><p>{text}</p></div></div>)}</div></Sample></div>;
}

function SenderSample() {
  return <div className="samples sc-content"><Sample caption="Initials in a rounded square"><Avatar kind="sender" name="Ada Example" /></Sample></div>;
}

function MemberSample() {
  return <div className="samples sc-content"><Sample caption="Extra small, control radius"><Avatar kind="member" name="Sam Fixture" /></Sample></div>;
}

function NavigationSample() {
  const [selected, setSelected] = useState('PRs');
  return <div className="samples sc-content"><Sample caption="Current destination and two others"><nav className="sc-glass-band" aria-label="Fixture destinations">{[
    { label: 'PRs', icon: GitPullRequest }, { label: 'Issues', icon: Hash }, { label: 'Channels', icon: MessageSquare },
  ].map(({ label, icon: Icon }) => <Button key={label} className="sc-nav-button" variant={selected === label ? 'selected' : 'ghost'} aria-current={selected === label ? 'page' : undefined} onClick={() => setSelected(label)}><Icon aria-hidden="true" />{label}</Button>)}</nav></Sample></div>;
}

function GlassControl({ initial }: { initial: boolean }) {
  const [selected, setSelected] = useState(initial);
  return <div className="sc-glass-band"><Button variant={selected ? 'selected' : 'outline'} className="sc-nav-button" aria-pressed={selected} onClick={() => setSelected(!selected)}>Search</Button></div>;
}

function GlassSamples() {
  return <div className="samples sc-content"><Sample caption="Rest"><GlassControl initial={false} /></Sample><Sample caption="Latched"><GlassControl initial /></Sample></div>;
}

function OverflowSample() {
  const [status, setStatus] = useState('');
  const popup = useRef<HTMLDetailsElement>(null);
  const trigger = useRef<HTMLElement>(null);
  function request(label: string) { setStatus(`${label} requested locally. No service action runs.`); if (popup.current) popup.current.open = false; trigger.current?.focus(); }
  return <div className="samples sc-content"><Sample caption="Too narrow for five actions; open the chevron"><div className="sc-overflow-sample">
    <div className="sc-overflow-toolbar"><Button variant="outline" onClick={() => request('Refresh')}><RefreshCw aria-hidden="true" />Refresh</Button>
      <Button variant="outline" onClick={() => request('Filter')}><Filter aria-hidden="true" />Filter</Button>
      <details ref={popup} className="sc-popup" onKeyDown={event => { if (event.key === 'Escape') { event.preventDefault(); if (popup.current) popup.current.open = false; trigger.current?.focus(); } }}>
        <summary ref={trigger} aria-label="More toolbar actions"><ChevronDown aria-hidden="true" /></summary><div className="sc-popup-body" role="group" aria-label="More toolbar actions">
          {[{ label: 'Sort', icon: ArrowUpDown }, { label: 'Share', icon: Share2 }, { label: 'Export', icon: Download }].map(({ label, icon: Icon }) => <Button variant="ghost" key={label} onClick={() => request(label)}><Icon aria-hidden="true" />{label}</Button>)}
        </div></details></div><span className="sc-note" role="status">{status}</span>
  </div></Sample></div>;
}

function SurfaceHeaderSample() {
  const [closed, setClosed] = useState(false);
  const [error, setError] = useState('');
  const [status, setStatus] = useState('');
  async function copy() {
    try { await navigator.clipboard.writeText('Fixture records'); setStatus('Copied fixture records'); setError(''); }
    catch { setError('Clipboard unavailable. Fixture records: no real logs are loaded.'); }
  }
  return <div className="samples sc-content"><Sample caption="Title, description, an action and Close"><div className="sc-header-sample">
    {closed ? <Button variant="outline" onClick={() => setClosed(false)}>Show fixture header</Button> : <header className="sc-surface-header"><ScrollText aria-hidden="true" /><div><strong>Logs</strong><span className="sc-note">Fixture records</span></div>
      <Button variant="outline" onClick={copy}><Copy aria-hidden="true" />Copy</Button><Button variant="ghost" size="icon" aria-label="Close logs" onClick={() => setClosed(true)}><X aria-hidden="true" /></Button></header>}
    {error && <Alert variant="destructive"><AlertDescription>{error}</AlertDescription></Alert>}<span className="sc-note" role="status">{status}</span>
  </div></Sample></div>;
}

const surfaceTokens = [
  ['bg', '--background'], ['pane', '--muted-background'], ['surface', '--popover-background'], ['chrome', '--title_bar-background'],
  ['sidebar', '--sidebar-background'], ['select', '--accent-background'], ['line', '--border'], ['hair', '--list-hover-background'],
];
const toneTokens = [
  ['fg', '--foreground'], ['fg_strong', '--caret'], ['muted', '--muted-foreground'], ['accent', '--primary-background'],
  ['red', '--danger-background'], ['orange', '--sc-native-orange'], ['yellow', '--warning-background'], ['green', '--success-background'],
  ['cyan', '--base-cyan'], ['blue', '--base-blue'], ['magenta', '--base-magenta'],
];

function TokenSample() {
  return <div className="samples sc-content">{[{ caption: 'Surfaces and lines', tokens: surfaceTokens }, { caption: 'Text and tones', tokens: toneTokens }].map(({ caption, tokens }) => <Sample key={caption} caption={caption}>
    <div className="sc-swatches">{tokens.map(([name, token]) => <div key={name} className="sc-swatch"><span aria-hidden="true" style={{ background: `var(${token})` }} /><code>{name}</code></div>)}</div>
  </Sample>)}</div>;
}

const fixtureGroups = [
  { name: 'example/component-gallery', rows: [{ id: '#12', title: 'Add a copy button to every card' }, { id: '#13', title: 'Hide unused GPUI Kit components' }] },
  { name: 'example/fixtures', rows: [{ id: '#4', title: 'Refresh the fixture data' }, { id: '#5', title: 'Name every sample state', muted: true }] },
];

function FixtureList({ notes = false }: { notes?: boolean }) {
  const [collapsed, setCollapsed] = useState<string[]>([]);
  const [selected, setSelected] = useState('');
  const [status, setStatus] = useState('');
  const refs = useRef<Record<string, HTMLButtonElement | null>>({});
  const groups = notes ? [{ name: 'example/notes', rows: [{ id: '#21', title: 'Explain an empty group' }] }, { name: 'example/loading', rows: [] }, { name: 'example/archive', rows: [] }] : fixtureGroups;
  const visibleIds = groups.filter(group => !collapsed.includes(group.name)).flatMap(group => group.rows.map(row => row.id));
  function move(event: KeyboardEvent<HTMLButtonElement>, id: string) {
    const ix = visibleIds.indexOf(id);
    const next = event.key === 'ArrowDown' ? Math.min(ix + 1, visibleIds.length - 1) : event.key === 'ArrowUp' ? Math.max(ix - 1, 0) : event.key === 'Home' ? 0 : event.key === 'End' ? visibleIds.length - 1 : null;
    if (next !== null) { event.preventDefault(); setSelected(visibleIds[next]); refs.current[visibleIds[next]]?.focus(); }
  }
  return <div className="sc-list"><div className="sc-list-rows" aria-label={notes ? 'Fixture list with group notes' : 'Fixture list'}>{groups.map(group => <section key={group.name}>
    <Button variant="ghost" className="sc-list-group" aria-expanded={!collapsed.includes(group.name)} onClick={() => setCollapsed(collapsed.includes(group.name) ? collapsed.filter(name => name !== group.name) : [...collapsed, group.name])}>
      {collapsed.includes(group.name) ? <ChevronRight aria-hidden="true" /> : <ChevronDown aria-hidden="true" />}<span className="sc-group-heading">▣ {group.name}</span><span className="sc-badge">{group.rows.length}</span>
    </Button>
    {!collapsed.includes(group.name) && <>{group.rows.map(row => <Button key={row.id} ref={node => { refs.current[row.id] = node; }} variant={selected === row.id ? 'selected' : 'ghost'} className={`sc-list-row ${'muted' in row && row.muted ? 'sc-muted' : ''}`}
      aria-pressed={selected === row.id} onClick={() => setSelected(row.id)} onDoubleClick={() => setStatus(`Fixture activation requested for ${row.id}. No external record opens.`)} onKeyDown={event => move(event, row.id)}>
      <span className="sc-row-dot" aria-hidden="true">●</span><span>{row.id}</span><span className="sc-row-title">{row.title}</span></Button>)}
      {notes && !group.rows.length && <p className="sc-list-note">{group.name === 'example/loading' ? 'Loading fixture rows…' : 'No open fixture rows'}</p>}</>}
  </section>)}</div><span role="status" className="sc-note">{status}</span></div>;
}

function ListSamples() {
  return <div className="samples sc-content"><Sample caption="Two groups of fixture rows, one muted"><FixtureList /></Sample><Sample caption="Group notes: loading and empty"><FixtureList notes /></Sample></div>;
}

const gridRows = [['github', 'fixture', 12], ['actions', 'fixture', 7.5], ['irc', 'fixture', 3]] as const;
const gridColumns = ['Board', 'Kind', 'Count'];
const gridOptions = ['Shaded header', 'Alternating rows', 'Cell boundaries', 'Row hover', 'Sorting', 'Column filters', 'Cell selection', 'Clipboard copy'];

function GridSample() {
  const [filters, setFilters] = useState(['', '', '']);
  const [sort, setSort] = useState<{ column: number; direction: 1 | -1 } | null>(null);
  const [options, setOptions] = useState(gridOptions.map(() => true));
  const [selected, setSelected] = useState<string[]>([]);
  const [anchor, setAnchor] = useState<[number, number] | null>(null);
  const [active, setActive] = useState<[number, number] | null>(null);
  const [status, setStatus] = useState('');
  const [optionsOpen, setOptionsOpen] = useState(false);
  const cells = useRef<Record<string, HTMLButtonElement | null>>({});
  const ordered = gridRows.map((row, ix) => ({ row, ix }));
  if (sort) ordered.sort((a, b) => (typeof a.row[sort.column] === 'number' ? Number(a.row[sort.column]) - Number(b.row[sort.column]) : String(a.row[sort.column]).localeCompare(String(b.row[sort.column]))) * sort.direction);
  const shown = ordered.filter(({ row }) => row.every((cell, column) => String(cell).toLowerCase().includes(filters[column].toLowerCase())));
  function select(row: number, column: number, extend: boolean) {
    if (!options[6]) return;
    const start: [number, number] = extend && anchor && shown.some(item => item.ix === anchor[0]) ? anchor : [row, column];
    setAnchor(start);
    const from = shown.findIndex(item => item.ix === start[0]);
    const to = shown.findIndex(item => item.ix === row);
    setSelected(shown.slice(Math.min(from, to), Math.max(from, to) + 1).flatMap(item => gridColumns.flatMap((_, col) => col >= Math.min(start[1], column) && col <= Math.max(start[1], column) ? [`${item.ix}:${col}`] : [])));
    setActive([row, column]);
  }
  async function copy(scope: 'selected' | 'view' | 'all') {
    const rows = scope === 'all' ? gridRows.map((row, ix) => ({ row, ix })) : scope === 'selected' ? ordered : shown;
    const text = rows.map(({ row, ix }) => row.filter((_, col) => scope !== 'selected' || selected.includes(`${ix}:${col}`)).join('\t')).filter(Boolean);
    if (scope !== 'selected') text.unshift(gridColumns.join('\t'));
    try { await navigator.clipboard.writeText(text.join('\n')); setStatus('Copied'); }
    catch { setStatus('Clipboard unavailable. Select and copy fixture values manually.'); }
  }
  function keyDown(event: KeyboardEvent<HTMLButtonElement>, row: number, column: number) {
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'c' && options[7] && selected.length) { event.preventDefault(); void copy('selected'); return; }
    const ix = shown.findIndex(item => item.ix === row);
    const nextRow = event.key === 'ArrowDown' ? Math.min(shown.length - 1, ix + 1) : event.key === 'ArrowUp' ? Math.max(0, ix - 1) : ix;
    const nextCol = event.key === 'ArrowRight' ? Math.min(2, column + 1) : event.key === 'ArrowLeft' ? Math.max(0, column - 1) : event.key === 'Home' ? 0 : event.key === 'End' ? 2 : column;
    if (!['ArrowDown', 'ArrowUp', 'ArrowRight', 'ArrowLeft', 'Home', 'End'].includes(event.key)) return;
    event.preventDefault(); select(shown[nextRow].ix, nextCol, event.shiftKey); cells.current[`${shown[nextRow].ix}:${nextCol}`]?.focus();
  }
  function toggleOption(ix: number) {
    setOptions(options.map((value, option) => option === ix ? !value : value));
    if (ix === 4) setSort(null);
    if (ix === 5) setFilters(['', '', '']);
    if (ix === 6) { setSelected([]); setAnchor(null); setActive(null); }
  }
  return <div className="samples sc-content"><Sample caption="3 × 3 fixture"><div className="sc-grid-sample">
    <span>Fixture grid</span><div className="sc-grid-toolbar">
      <Button variant="ghost" disabled={!sort} onClick={() => setSort(null)}>Clear sort</Button>
      <Button variant="ghost" disabled={filters.every(value => !value)} onClick={() => setFilters(['', '', ''])}>Clear filters</Button>
      <details><summary>Copy</summary><div className="sc-grid-copy">{(['selected', 'view', 'all'] as const).map(scope => <Button key={scope} variant="ghost" disabled={!options[7] || (scope === 'selected' && !selected.length)} onClick={() => void copy(scope)}>Copy {scope}</Button>)}</div></details>
      <Button variant={optionsOpen ? 'selected' : 'ghost'} aria-expanded={optionsOpen} onClick={() => setOptionsOpen(!optionsOpen)}>Options</Button>
      <Button variant="ghost" disabled title="The native fixture has no export owner; this web sample writes no files."><Download aria-hidden="true" />Export…</Button>
    </div>
    {optionsOpen && <div className="sc-grid-options">{gridOptions.map((label, ix) => <label key={label}>{label}<input type="checkbox" role="switch" checked={options[ix]} onChange={() => toggleOption(ix)} /></label>)}</div>}
    <div className={`sc-grid-scroll ${options[0] ? 'sc-grid-shaded' : ''} ${options[1] ? 'sc-grid-striped' : ''} ${options[2] ? 'sc-grid-lines' : ''} ${options[3] ? 'sc-grid-hover' : ''}`}>
      <table aria-label="Fixture grid"><thead><tr>{gridColumns.map((name, column) => <th key={name} aria-sort={sort?.column === column ? sort.direction === 1 ? 'ascending' : 'descending' : 'none'}><Button variant="ghost" disabled={!options[4]} onClick={() => setSort(sort?.column !== column ? { column, direction: 1 } : sort.direction === 1 ? { column, direction: -1 } : null)}><ArrowUpDown aria-hidden="true" />{name}{sort?.column === column && (sort.direction === 1 ? <ArrowUp aria-label="ascending" /> : <ArrowDown aria-label="descending" />)}</Button></th>)}</tr>
        {options[5] && <tr>{gridColumns.map((name, column) => <td key={name}><Input aria-label={`Filter ${name}`} placeholder="Contains…" value={filters[column]} onChange={event => setFilters(filters.map((value, ix) => ix === column ? event.target.value : value))} /></td>)}</tr>}</thead>
        <tbody>{shown.map(({ row, ix }) => <tr key={row[0]}>{row.map((value, col) => <td key={gridColumns[col]}><button type="button" className={`sc-grid-cell ${col === 2 ? 'sc-numeric' : ''}`} disabled={!options[6]}
          ref={node => { cells.current[`${ix}:${col}`] = node; }} aria-pressed={selected.includes(`${ix}:${col}`)} data-active={active?.[0] === ix && active[1] === col}
          onClick={event => select(ix, col, event.shiftKey)} onKeyDown={event => keyDown(event, ix, col)}>{value}</button></td>)}</tr>)}</tbody></table>
      {!shown.length && <p className="sc-note">No matching rows. Clear filters to show all results.</p>}
    </div><span role="status" className="sc-note">{shown.length} of 3 rows · {selected.length} cells selected · {status}</span>
    <span className="sc-note">Shift+click or Shift+arrows extends selection. Export needs the native host.</span>
  </div></Sample></div>;
}

function ChartSample() {
  return <div className="samples sc-content"><Sample caption="Bar"><figure className="sc-chart"><figcaption>Fixture builds per day</figcaption>
    <div className="sc-chart-plot" role="img" aria-label="Bar chart: Mon 4 builds, Tue 9 builds, Wed 6 builds. Value axis: builds. Category axis: day.">
      <span className="sc-chart-axis">builds</span><div className="sc-chart-bars">{[{ day: 'Mon', value: 4 }, { day: 'Tue', value: 9 }, { day: 'Wed', value: 6 }].map(({ day, value }) => <div className="sc-chart-column" key={day}>
        <span className="sc-chart-bar" style={{ '--bar-value': value } as CSSProperties} title={`${day}: ${value} builds`}><span>{value}</span></span><span>{day}</span>
      </div>)}</div></div><span className="sc-chart-axis-label">day · ■ builds</span>
  </figure></Sample></div>;
}

function SegmentedFixture({ labels }: { labels: string[] }) {
  const [selected, setSelected] = useState(labels[0]);
  return <div className="sc-segmented" role="group" aria-label="Fixture choice">{labels.map(label => <Button key={label} variant={selected === label ? 'selected' : 'ghost'} aria-pressed={selected === label} onClick={() => setSelected(label)}>{label}</Button>)}</div>;
}

function ModeSwitchSample() {
  return <div className="samples sc-content"><Sample caption="Shared: segmented::control"><SegmentedFixture labels={['Approve', 'Request changes', 'Comment']} /></Sample></div>;
}

function RowActionsSample() {
  return <div className="samples sc-content"><Sample caption="Shared: segmented::control"><SegmentedFixture labels={['Keep', 'Replace', 'Clear']} /></Sample></div>;
}

function CheckRunSample() {
  return <div className="samples sc-content"><Sample caption="Shared: links::external"><External>Check run</External></Sample></div>;
}

function ConfirmationSample() {
  const [status, setStatus] = useState('');
  return <div className="samples sc-content"><Sample caption="Shared: dialog::frame with dialog::footer"><section className="sc-confirmation" aria-label="Remove the fixture board?">
    <h3>Remove the fixture board?</h3><p className="sc-note">This removes the fixture only.</p><div className="sc-confirmation-footer">
      <Button variant="outline" onClick={() => setStatus('Keep board requested. The fixture is unchanged.')}><X aria-hidden="true" />Keep board</Button>
      <Button variant="destructive" onClick={() => setStatus('Remove board requested locally. No installed Space is removed.')}><Trash2 aria-hidden="true" />Remove board</Button>
    </div><span role="status" className="sc-note">{status}</span>
  </section></Sample></div>;
}

function SizeStripSample() {
  return <div className="samples sc-content"><Sample caption="replica: badge::badge 16/7 px"><span className="sc-size-replica sc-size-badge">Aa</span></Sample>
    <Sample caption="replica: github::mode_switch 20/4 px"><span className="sc-size-replica sc-size-mode">Aa</span></Sample></div>;
}

type ShellSurface = { action?: string; components: string[] };
const shellSurfaces: Record<string, ShellSurface> = {
  'shell::window': { components: ['gpui_kit::component::resizable::ResizablePanelGroup', 'gpui_kit::component::Root'] },
  'shell::toolbar': { components: ['glass::nav_button', 'overflow_toolbar::OverflowToolbar', 'icon_button::IconButton', 'badge::badge'] },
  'shell::navigation_rail': { action: 'subspace_app::ToggleNavigationMode', components: ['glass::nav_button', 'badge::badge'] },
  'shell::status_bar': { components: ['icon_button::IconButton', 'gpui_kit::component::spinner::Spinner', 'gpui_kit::component::status_bar::StatusBar'] },
  'shell::command_palette': { action: 'subspace_app::TogglePalette', components: ['gpui_kit::component::command::Command', 'badge::badge'] },
  'shell::help': { action: 'subspace_app::ToggleHelp', components: ['rich::rich_text', 'gpui_kit::component::collapsible::Collapsible', 'gpui_kit::component::dock::DockArea'] },
  'shell::settings_dialog': { action: 'subspace_app::OpenSettings', components: ['fields::form_field', 'fields::heading', 'fields::note', 'toggle::toggle_row', 'secret::secret_field', 'anchored::select', 'gpui_kit::component::tree::Tree', 'gpui_kit::component::dialog::AlertDialog'] },
  'shell::about_dialog': { action: 'subspace_app::OpenAbout', components: ['copy_details::CopyDetails', 'links::external', 'gpui_kit::component::group_box::GroupBox', 'gpui_kit::component::progress::Progress', 'gpui_kit::component::dialog::Dialog'] },
  'shell::report_issue': { action: 'subspace_app::OpenReportIssue', components: ['toggle::toggle_row', 'gpui_kit::component::input::Textarea', 'action::action_button', 'gpui_kit::component::collapsible::Collapsible'] },
  'shell::theme_designer': { action: 'subspace_app::ToggleJig', components: ['gpui_kit::component::accordion::Accordion', 'segmented::control', 'toggle::toggle_row', 'gpui_kit::component::slider::Slider', 'tokens::Tokens'] },
  'shell::toasts': { components: ['Toast', 'action::icon_button', 'links::external'] },
  'shell::chat_panel': { action: 'subspace_app::ToggleChat', components: ['chat_input::ChatInput', 'chat_input::completion::CompletionList', 'chat_bubble::bubble', 'tabs::tab_bar', 'gpui_kit::component::attachment::Attachment'] },
  'shell::chat_canvas': { components: ['chart::Chart', 'action::action_button'] },
  'shell::search_view': { action: 'subspace_app::OpenSearchView', components: ['surface_header::SurfaceHeader', 'fields::input', 'icon_button::latched', 'toggle::switch'] },
  'shell::log_view': { action: 'subspace_app::OpenLogView', components: ['surface_header::SurfaceHeader', 'tabs::tab_bar', 'overflow_toolbar::OverflowToolbar', 'icon_button::latched'] },
};

function ShellSample({ name }: { name: string }) {
  const surface = shellSurfaces[name];
  const [status, setStatus] = useState('');
  function reveal(id: string) {
    // The catalogue owner handles filtering/selection; samples cannot own it.
    // Canonical card IDs are carried verbatim, including GPUI Kit prefixes.
    const handled = !window.dispatchEvent(new CustomEvent<string>('gallery-reveal-card', { detail: id, cancelable: true }));
    if (handled) return;
    // Standalone fallback preserves theme and other URL options. An owner can
    // preventDefault on the event above for same-document selection instead.
    const card = Object.values(cards).find(item => item.name === id);
    if (!card) { setStatus(`Component is not registered: ${id}`); return; }
    const url = new URL(window.location.href);
    url.searchParams.set('card', card.id);
    window.location.assign(url.href);
  }
  return <div className="samples sc-content sc-shell">
    {surface.action && <Sample caption="Opens the real surface (native host only)"><Button variant="outline" disabled title={`Native-only command: ${surface.action}`}><ExternalLink aria-hidden="true" />Open</Button>
      <p className="sc-note">{surface.action} is unavailable in this web gallery.</p></Sample>}
    <Sample caption="Built from; click to show each card"><div className="sc-built-from">{surface.components.map(component => <Button key={component} variant="ghost" className="sc-component-link" onClick={() => reveal(component)}>{component}</Button>)}</div></Sample>
    <p className="sc-note">The native gallery links to these components; it does not render a duplicate shell surface.</p>
    <span className="sr-only" role="status">{status}</span>
  </div>;
}

export const subspaceContentSamples: Record<string, ComponentType> = {
  'badge::badge': BadgeSamples,
  'links::external': ExternalSample,
  'links::external_marked': MarkedExternalSample,
  'truncated::text': TruncatedSamples,
  'rich::plain_text': PlainTextSample,
  'rich::rich_text': RichTextSamples,
  'typography::group_heading': GroupHeadingSample,
  'toolbar_label::label': ToolbarLabelSample,
  'avatar::person': PersonSample,
  'avatar::group': GroupSamples,
  'chat_input::ChatInput': ChatInputSamples,
  'chat_input::completion::CompletionList': CompletionSample,
  'composer::Composer': ComposerSample,
  'chat_bubble::bubble': BubbleSample,
  'chat_bubble::message': MessageSample,
  'avatar::sender': SenderSample,
  'avatar::member': MemberSample,
  'glass::nav_button': NavigationSample,
  'glass::control': GlassSamples,
  'overflow_toolbar::OverflowToolbar': OverflowSample,
  'surface_header::SurfaceHeader': SurfaceHeaderSample,
  'tokens::Tokens': TokenSample,
  'list::render': ListSamples,
  'native_grid::NativeGrid': GridSample,
  'chart::Chart': ChartSample,
  'github::mode_switch': ModeSwitchSample,
  'board-lab::row_actions': RowActionsSample,
  'github::check_run_link': CheckRunSample,
  'inline-confirmations': ConfirmationSample,
  'inconsistencies::size_and_radius_strip': SizeStripSample,
  'shell::window': () => <ShellSample name="shell::window" />,
  'shell::toolbar': () => <ShellSample name="shell::toolbar" />,
  'shell::navigation_rail': () => <ShellSample name="shell::navigation_rail" />,
  'shell::status_bar': () => <ShellSample name="shell::status_bar" />,
  'shell::command_palette': () => <ShellSample name="shell::command_palette" />,
  'shell::help': () => <ShellSample name="shell::help" />,
  'shell::settings_dialog': () => <ShellSample name="shell::settings_dialog" />,
  'shell::about_dialog': () => <ShellSample name="shell::about_dialog" />,
  'shell::report_issue': () => <ShellSample name="shell::report_issue" />,
  'shell::theme_designer': () => <ShellSample name="shell::theme_designer" />,
  'shell::toasts': () => <ShellSample name="shell::toasts" />,
  'shell::chat_panel': () => <ShellSample name="shell::chat_panel" />,
  'shell::chat_canvas': () => <ShellSample name="shell::chat_canvas" />,
  'shell::search_view': () => <ShellSample name="shell::search_view" />,
  'shell::log_view': () => <ShellSample name="shell::log_view" />,
};
