import { useId, useRef, useState, type ComponentType, type KeyboardEvent, type ReactNode } from 'react';
import { createPortal } from 'react-dom';
import { Bold, Folder, GitBranch, Italic, MessageSquare, Redo2, Undo2, X } from 'lucide-react';
import { Button } from './ui/button';
import { Input } from './ui/input';
import { Alert, AlertDescription, AlertTitle } from './ui/alert';
import './kit-regions.css';

// React owns each fixture's transient state. Native samples live in
// samples/kit/layout/{regions,overlays}.rs; no native service is called.
function Sample({ caption, children }: { caption: string; children: ReactNode }) {
  return <div className="kr-sample">{children}<div className="sample-caption">{caption}</div></div>;
}

function DescriptionListSample() {
  return <Sample caption="Bordered, two columns"><dl className="kr-description">
    {[['Name', 'Sample fixture'], ['Owner', 'Ada Example'], ['Board', 'component-gallery'], ['Updated', 'Today']].map(([label, value]) =>
      <div key={label}><dt>{label}</dt><dd>{value}</dd></div>)}
  </dl></Sample>;
}

function GroupBoxSample() {
  return <div className="kr-row">{['Titled', 'Outline'].map(style => <Sample key={style} caption={style}>
    <fieldset className={`kr-group ${style === 'Outline' ? 'kr-group-outline' : ''}`}>
      <legend>Updates</legend>Related settings grouped together.
    </fieldset>
  </Sample>)}</div>;
}

type Notice = { tone: 'info' | 'success' | 'warning' | 'error'; text: string; title?: string };
function NoticeView({ notice, dismiss }: { notice: Notice; dismiss: () => void }) {
  return createPortal(<div className={`kr-notice kr-tone-${notice.tone}`}>
    <span aria-hidden="true">{({ info: 'ⓘ', success: '✓', warning: '⚠', error: '⊗' })[notice.tone]}</span>
    <Alert variant={notice.tone === 'error' ? 'destructive' : 'default'}>
      {notice.title && <AlertTitle>{notice.title}</AlertTitle>}
      <AlertDescription>{notice.text}</AlertDescription>
    </Alert>
    <Button variant="ghost" size="icon" aria-label="Dismiss notification" onClick={dismiss}><X /></Button>
  </div>, document.body);
}

function RootSample() {
  const [notice, setNotice] = useState(false);
  return <Sample caption="This window's Root shows the notification">
    <Button variant="outline" onClick={() => setNotice(true)}>Notify through Root</Button>
    {notice && <NoticeView notice={{ tone: 'info', text: "Root hosts this window's dialogs, sheets and notifications." }} dismiss={() => setNotice(false)} />}
  </Sample>;
}

function ThemeSample() {
  const swatches = ['background', 'foreground', 'primary', 'secondary', 'muted', 'accent', 'border', 'popover', 'sidebar', 'info', 'success', 'warning', 'danger', 'ring'];
  const single = new Set(['background', 'foreground', 'border', 'ring']);
  return <Sample caption="The active theme's semantic colours"><div className="kr-swatches">
    {swatches.map(name => <div key={name}><span style={{ background: `var(--${name}${single.has(name) ? '' : '-background'})` }} />{name}</div>)}
  </div></Sample>;
}

function ModalSample({ alert = false, sheet = false }: { alert?: boolean; sheet?: boolean }) {
  const dialog = useRef<HTMLDialogElement>(null);
  const cancel = useRef<HTMLButtonElement>(null);
  const title = useId();
  const description = useId();
  const [outcome, setOutcome] = useState('');
  const opener = sheet ? 'Open sheet…' : alert ? 'Delete fixture…' : 'Open dialog…';
  return <Sample caption={sheet ? 'Slides in from the edge' : alert ? 'Asks before a consequential action' : 'Opens a dialog with Cancel and OK'}>
    <Button variant="outline" onClick={() => { if (dialog.current) { dialog.current.returnValue = ''; dialog.current.showModal(); if (alert) cancel.current?.focus(); } }}>{opener}</Button>
    <dialog ref={dialog} className={`kr-modal ${sheet ? 'kr-sheet' : ''}`} role={alert ? 'alertdialog' : 'dialog'} aria-labelledby={title} aria-describedby={description}
      onKeyDown={event => {
        if (event.key !== 'Tab') return;
        const controls = event.currentTarget.querySelectorAll<HTMLButtonElement>('button:not(:disabled)');
        const first = controls[0], last = controls[controls.length - 1];
        if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last?.focus(); }
        else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first?.focus(); }
      }}
      onClose={() => { if (dialog.current?.returnValue === 'confirm') setOutcome(alert ? 'Sample fixture deleted from this demonstration.' : 'Fixture renamed.'); }}>
      <form method="dialog">
        <header><h3 id={title}>{sheet ? 'Fixture details' : alert ? 'Delete “Sample fixture”?' : 'Rename fixture'}</h3>
          <Button variant="ghost" size="icon" aria-label="Close" value="cancel" type="submit"><X /></Button></header>
        <p id={description}>{sheet ? 'Supplementary work that needs more room than a popover.' : alert ? 'The fixture is removed from the gallery until it reloads.' : 'A dialog holds one focused decision or a short form.'}</p>
        <div className="kr-modal-actions">
          <Button ref={cancel} variant="outline" type="submit" value="cancel">{sheet ? 'Close' : 'Cancel'}</Button>
          {!sheet && <Button variant={alert ? 'destructive' : 'default'} type="submit" value="confirm">{alert ? 'Delete' : 'Rename'}</Button>}
        </div>
      </form>
    </dialog>
    {outcome && <output className="kr-muted">{outcome}</output>}
  </Sample>;
}

function DialogSample() { return <ModalSample />; }
function AlertDialogSample() { return <ModalSample alert />; }
function SheetSample() { return <ModalSample sheet />; }

function NotificationSample() {
  const [notice, setNotice] = useState<Notice | null>(null);
  const notices: Notice[] = [
    { tone: 'info', text: 'Fixture synced.' },
    { tone: 'success', text: 'Fixture saved.' },
    { tone: 'warning', text: 'The fixture token expires soon.' },
    { tone: 'error', title: 'Connection failed', text: 'Couldn’t reach example.com.' },
  ];
  return <Sample caption="In-app; each button shows one type"><div className="kr-row">
    {notices.map(item => <Button key={item.tone} variant="outline" onClick={() => setNotice(item)}>{item.tone[0].toUpperCase() + item.tone.slice(1)}</Button>)}
  </div>{notice && <NoticeView notice={notice} dismiss={() => setNotice(null)} />}</Sample>;
}

function PopoverSample() {
  const id = useId();
  return <Sample caption="Through anchored::Anchor; click to open"><div className="kr-popover-host">
    <Button variant="outline" popoverTarget={id}><MessageSquare />Show popover</Button>
    <div id={id} popover="auto" className="kr-popover"><p>Fixture popover</p><p className="kr-muted">Contextual content that does not interrupt the task. Press Escape to close.</p>
      <Button variant="outline" popoverTarget={id} popoverTargetAction="hide">Close</Button>
    </div>
  </div></Sample>;
}

function HoverCardSample() {
  const [dismissed, setDismissed] = useState(false);
  const id = useId();
  return <Sample caption="Hover the name"><div className={`kr-hover-host ${dismissed ? 'kr-hover-dismissed' : ''}`}
    onMouseEnter={() => setDismissed(false)} onFocus={() => setDismissed(false)} onKeyDown={event => { if (event.key === 'Escape') setDismissed(true); }}>
    <Button variant="ghost" aria-describedby={id}>Ada Example</Button>
    <div id={id} className="kr-hover-card" role="tooltip"><strong>Ada Example</strong><span className="kr-muted">Fixture person · example.com</span></div>
  </div></Sample>;
}

function ResizableSample() {
  const [width, setWidth] = useState(140);
  const group = useRef<HTMLDivElement>(null);
  const resize = (value: number) => setWidth(Math.min(300, Math.max(80, value)));
  return <Sample caption="Two panes"><div ref={group} className="kr-resizable">
    <div style={{ width }} className="kr-resizable-first">Drag the edge</div>
    <div role="separator" tabIndex={0} aria-label="Pane width" aria-orientation="vertical" aria-valuemin={80} aria-valuemax={300} aria-valuenow={Math.round(width)} className="kr-resize-handle"
      onKeyDown={event => { if (['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) { event.preventDefault(); resize(event.key === 'Home' ? 80 : event.key === 'End' ? 300 : width + (event.key === 'ArrowLeft' ? -10 : 10)); } }}
      onPointerDown={event => { event.currentTarget.setPointerCapture(event.pointerId); }}
      onPointerMove={event => { if (event.currentTarget.hasPointerCapture(event.pointerId) && group.current) resize(event.clientX - group.current.getBoundingClientRect().left); }}
      onPointerUp={event => event.currentTarget.releasePointerCapture(event.pointerId)} />
    <div className="kr-resizable-rest">The rest</div>
  </div></Sample>;
}

function ScrollableSample() {
  return <Sample caption="Vertical scrollbar"><div className="kr-scroll-lines" tabIndex={0} aria-label="24 fixture lines">
    {Array.from({ length: 24 }, (_, ix) => <div key={ix}>Fixture line {ix + 1}</div>)}
  </div></Sample>;
}

function SidebarSample() {
  const [active, setActive] = useState('Inbox');
  return <Sample caption="Click a destination"><nav className="kr-sidebar" aria-label="Mail">
    <div className="kr-muted">Mail</div>{['Inbox', 'Drafts', 'Archive'].map(label =>
      <Button variant={active === label ? 'selected' : 'ghost'} key={label} aria-current={active === label ? 'page' : undefined} onClick={() => setActive(label)}><Folder />{label}</Button>)}
  </nav></Sample>;
}

function StatusBarSample() {
  return <Sample caption="Leading and trailing regions"><div className="kr-status" role="status">
    <span><GitBranch aria-hidden="true" /> main</span><span>Ln 12, Col 4</span>
  </div></Sample>;
}

function ToolbarSample() {
  const [formats, setFormats] = useState({ bold: false, italic: false });
  const [history, setHistory] = useState<typeof formats[]>([]);
  const [future, setFuture] = useState<typeof formats[]>([]);
  const toggle = (key: keyof typeof formats) => { setHistory([...history, formats]); setFormats({ ...formats, [key]: !formats[key] }); setFuture([]); };
  const undo = () => { const previous = history.at(-1); if (previous) { setFuture([...future, formats]); setFormats(previous); setHistory(history.slice(0, -1)); } };
  const redo = () => { const next = future.at(-1); if (next) { setHistory([...history, formats]); setFormats(next); setFuture(future.slice(0, -1)); } };
  return <Sample caption="Grouped commands"><div className="kr-toolbar" role="toolbar" aria-label="Fixture formatting">
    <div role="group" aria-label="History"><span>History</span><Button variant="ghost" aria-label="Undo" disabled={!history.length} onClick={undo}><Undo2 /></Button><Button variant="ghost" aria-label="Redo" disabled={!future.length} onClick={redo}><Redo2 /></Button></div>
    <div role="group" aria-label="Format"><span>Format</span><Button variant={formats.bold ? 'selected' : 'ghost'} aria-label="Bold" aria-pressed={formats.bold} onClick={() => toggle('bold')}><Bold /></Button><Button variant={formats.italic ? 'selected' : 'ghost'} aria-label="Italic" aria-pressed={formats.italic} onClick={() => toggle('italic')}><Italic /></Button></div>
  </div><p className="kr-toolbar-preview" style={{ fontWeight: formats.bold ? 'bold' : undefined, fontStyle: formats.italic ? 'italic' : undefined }}>Sample fixture</p></Sample>;
}

function TitleBarSample() {
  return <Sample caption="Shown only: dragging would move the window"><div className="kr-titlebar">
    <span className="kr-window-dots" aria-hidden="true"><i /><i /><i /></span><span>Fixture window</span>
  </div></Sample>;
}

function FocusTrapSample() {
  const [trapped, setTrapped] = useState(true);
  const trap = useRef<HTMLDivElement>(null);
  function onKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    if (event.key === 'Escape') { setTrapped(false); return; }
    if (event.key !== 'Tab' || !trapped) return;
    const fields = trap.current?.querySelectorAll<HTMLInputElement>('input');
    if (!fields?.length) return;
    const first = fields[0], last = fields[fields.length - 1];
    if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last.focus(); }
    else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus(); }
  }
  return <Sample caption="Click a field, then Tab: focus stays inside"><div className="kr-focus-trap" ref={trap} onKeyDown={onKeyDown}>
    <Input aria-label="First trapped field" placeholder="First field" onClick={() => setTrapped(true)} />
    <Input aria-label="Second trapped field" placeholder="Second field" onClick={() => setTrapped(true)} />
  </div><small className="kr-muted">{trapped ? 'Escape releases focus.' : 'Focus released. Click a field to trap again.'}</small></Sample>;
}

export const kitLayoutSamples: Record<string, ComponentType> = {
  'gpui_kit::component::description_list::DescriptionList': DescriptionListSample,
  'gpui_kit::component::group_box::GroupBox': GroupBoxSample,
  'gpui_kit::component::Root': RootSample,
  'gpui_kit::component::Theme': ThemeSample,
  'gpui_kit::component::dialog::Dialog': DialogSample,
  'gpui_kit::component::dialog::AlertDialog': AlertDialogSample,
  'gpui_kit::component::notification::Notification': NotificationSample,
  'gpui_kit::component::popover::Popover': PopoverSample,
  'gpui_kit::component::hover_card::HoverCard': HoverCardSample,
  'gpui_kit::component::resizable::ResizablePanelGroup': ResizableSample,
  'gpui_kit::component::scroll::Scrollable': ScrollableSample,
  'gpui_kit::component::sheet::Sheet': SheetSample,
  'gpui_kit::component::sidebar::Sidebar': SidebarSample,
  'gpui_kit::component::status_bar::StatusBar': StatusBarSample,
  'gpui_kit::component::toolbar::Toolbar': ToolbarSample,
  'gpui_kit::component::TitleBar': TitleBarSample,
  'gpui_kit::component::FocusTrapElement': FocusTrapSample,
};
