import { useEffect, useId, useRef, useState, type ComponentType, type ReactNode } from 'react';
import { Bell, Check, ChevronDown, ChevronLeft, ChevronRight, ChevronsDown, CircleCheck, CircleX, Component, Copy, FileText, Inbox, Pin, Search, Star, TriangleAlert, User } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import './kit-controls.css';

function Sample({ caption, children, className = '' }: { caption: string; children: ReactNode; className?: string }) {
  return <div className={`sample kit-sample ${className}`}><div>{children}</div><div className="sample-caption">{caption}</div></div>;
}

function AccordionSample() {
  const [open, setOpen] = useState<number | null>(null);
  const id = useId();
  const sections = [
    ['What is a fixture?', 'Obviously fake data that a sample shows.'],
    ['Does it read files?', 'No. The gallery reads no configuration.'],
    ['Can I copy a name?', 'Yes, with the copy button on each card.'],
  ];
  return <div className="samples"><Sample caption="Interactive, one open at a time"><div className="kit-accordion">
    {sections.map(([title, body], index) => <div key={title}>
      <Button variant="ghost" aria-expanded={open === index} aria-controls={`${id}-${index}`} onClick={() => setOpen(open === index ? null : index)}>{title}<ChevronDown /></Button>
      <div id={`${id}-${index}`} hidden={open !== index} className="kit-disclosure-body">{body}</div>
    </div>)}
  </div></Sample></div>;
}

function Alerts() {
  const states = [
    ['Info', 'Read only', 'The fixture board is read only.', 'info'],
    ['Success', '', 'Fixture saved.', 'success'],
    ['Warning', '', 'The fixture token expires soon.', 'warning'],
    ['Error', 'Connection failed', 'Couldn’t reach example.com.', 'danger'],
    ['Banner', '', 'A banner spans its container.', 'info'],
  ];
  return <div className="samples">{states.map(([caption, title, text, tone]) => <Sample key={caption} caption={caption}>
    <Alert className={`kit-alert kit-tone-${tone} ${caption === 'Banner' ? 'kit-banner' : ''}`}>
      {tone === 'success' ? <CircleCheck /> : tone === 'warning' ? <TriangleAlert /> : tone === 'danger' ? <CircleX /> : <Bell />}
      <div>{title && <AlertTitle>{title}</AlertTitle>}<AlertDescription>{text}</AlertDescription></div>
    </Alert>
  </Sample>)}</div>;
}

function Attachments() {
  return <div className="samples">{[['Complete', '24 KB'], ['Uploading', 'Uploading…'], ['Failed', 'Upload failed']].map(([state, description]) => <Sample caption={state} key={state}>
    <div className={`kit-attachment ${state === 'Failed' ? 'kit-tone-danger' : ''}`}><FileText /><div><strong>fixture-report.pdf</strong><small>{description}</small>
      {state === 'Uploading' && <progress value={45} max={100} aria-label="Fixture upload" />}</div>{state === 'Complete' ? <Check /> : state === 'Failed' ? <CircleX /> : <Spinner />}</div>
  </Sample>)}</div>;
}

function Avatar({ name, small = false }: { name?: string; small?: boolean }) {
  return <span className={`kit-avatar ${small ? 'kit-avatar-small' : ''}`} role="img" aria-label={name ?? 'User placeholder'} title={name}>{name ? name.split(' ').map(word => word[0]).join('') : <User />}</span>;
}

function Avatars() {
  return <div className="samples">
    <Sample caption="Initials"><Avatar name="Ada Example" /></Sample>
    <Sample caption="Placeholder"><Avatar /></Sample>
    <Sample caption="Small"><Avatar name="Sam Fixture" small /></Sample>
    <Sample caption="Group with a limit"><div className="kit-avatar-group"><Avatar name="Ada Example" /><Avatar name="Sam Fixture" /><Avatar name="Lee Sample" /><span className="kit-avatar" role="img" aria-label="1 more: Kim Demo">+1</span></div></Sample>
  </div>;
}

function Badges() {
  return <div className="samples">{['Count', 'Over the maximum', 'Dot', 'Icon'].map((caption, index) => <Sample key={caption} caption={caption}>
    <span className="kit-badge-wrap" role="img" aria-label={['8 notifications', 'More than 99 notifications', 'Unread notifications', 'Inbox checked'][index]}>{index === 3 ? <Inbox /> : <Bell />}<span className={`kit-badge ${index === 2 ? 'kit-badge-dot' : ''}`}>{['8', '99+', '', <Check key="check" />][index]}</span></span>
  </Sample>)}</div>;
}

function Breadcrumbs() {
  return <div className="samples"><Sample caption="Path with a disabled step"><nav aria-label="Fixture breadcrumb" className="kit-row"><Button variant="ghost">Boards</Button><ChevronRight /><Button variant="ghost" disabled>Fixtures</Button><ChevronRight /><span aria-current="page">Component gallery</span></nav></Sample></div>;
}

function Bubbles() {
  return <div className="samples"><Sample caption="Primary"><div className="kit-bubble">A message I sent.</div></Sample><Sample caption="Secondary"><div className="kit-bubble kit-bubble-secondary">A message I received.</div></Sample></div>;
}

function Buttons() {
  const [period, setPeriod] = useState('Week');
  return <div className="samples">
    <Sample caption="Variants"><div className="kit-row"><Button>Primary</Button><Button variant="outline">Default</Button><Button variant="outline">Outline</Button><Button variant="ghost">Ghost</Button><Button variant="destructive">Delete</Button></div></Sample>
    <Sample caption="Icon, loading, disabled, selected"><div className="kit-row"><Button variant="outline"><Search />Search</Button><Button variant="outline" disabled aria-busy="true"><Spinner />Saving…</Button><Button variant="outline" disabled>Disabled</Button><Button variant="selected" aria-pressed="true">Selected</Button></div></Sample>
    <Sample caption="Button group"><div className="kit-segments" role="group" aria-label="Fixture period">{['Day', 'Week', 'Month'].map(label => <Button key={label} variant={period === label ? 'selected' : 'outline'} aria-pressed={period === label} onClick={() => setPeriod(label)}>{label}</Button>)}</div></Sample>
  </div>;
}

function Checkboxes() {
  const [checked, setChecked] = useState(true);
  return <div className="samples"><Sample caption="Interactive"><label className="kit-check"><input type="checkbox" checked={checked} onChange={event => setChecked(event.target.checked)} />Show hidden fixtures</label></Sample><Sample caption="Disabled"><label className="kit-check"><input type="checkbox" checked disabled readOnly />Locked option</label></Sample></div>;
}

function Clipboards() {
  const [status, setStatus] = useState('');
  async function copy() {
    try { await navigator.clipboard.writeText('fixture-value'); setStatus('Copied fixture-value'); }
    catch { setStatus('Clipboard unavailable. Select and copy fixture-value.'); }
  }
  return <div className="samples"><Sample caption="Copies a fixture value"><div className="kit-row"><span>fixture-value</span><Button variant="ghost" size="icon" aria-label="Copy value" title="Copy value" onClick={copy}><Copy /></Button></div><span className="kit-feedback" role="status">{status}</span></Sample></div>;
}

function Collapsibles() {
  const [open, setOpen] = useState(false);
  const id = useId();
  return <div className="samples"><Sample caption="Interactive; click Details"><div className="kit-disclosure"><Button variant="ghost" aria-expanded={open} aria-controls={id} onClick={() => setOpen(!open)}>{open ? <ChevronDown /> : <ChevronRight />}Details</Button><div className="kit-disclosure-body" id={id} hidden={!open}>Fixture details inside the disclosure.</div></div></Sample></div>;
}

function DropdownButtons() {
  // Native fixture configures no menu items. Do not invent export outcomes.
  return <div className="samples"><Sample caption="Main action and menu trigger"><div className="kit-segments"><Button variant="outline">Export</Button><Button variant="outline" aria-label="More export options (none configured in fixture)" disabled><ChevronDown /></Button></div></Sample></div>;
}

function Empties() {
  return <div className="samples"><Sample caption="Media, title, description and action"><div className="kit-empty"><Inbox /><strong>No fixtures</strong><p>Add a fixture to see it in this list.</p><Button variant="outline">Add fixture</Button></div></Sample></div>;
}

function Icons() {
  return <div className="samples"><Sample caption="Sizes"><div className="kit-row" aria-label="Component icon sizes">{['xs', 'sm', 'md', 'lg'].map(size => <Component key={size} className={`kit-icon-${size}`} aria-label={`${size} component icon`} />)}</div></Sample><Sample caption="Semantic colours"><div className="kit-row"><CircleCheck className="kit-tone-success" aria-label="Success" /><TriangleAlert className="kit-tone-warning" aria-label="Warning" /><CircleX className="kit-tone-danger" aria-label="Error" /></div></Sample></div>;
}

function Images() {
  // Original embedded artwork is content, not a screenshot of a component.
  return <div className="samples"><Sample caption="Embedded SVG"><svg role="img" aria-label="Component artwork" className="kit-artwork" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#57606a" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M15.536 11.293a1 1 0 0 0 0 1.414l2.376 2.377a1 1 0 0 0 1.414 0l2.377-2.377a1 1 0 0 0 0-1.414l-2.377-2.377a1 1 0 0 0-1.414 0z" /><path d="M2.297 11.293a1 1 0 0 0 0 1.414l2.377 2.377a1 1 0 0 0 1.414 0l2.377-2.377a1 1 0 0 0 0-1.414L6.088 8.916a1 1 0 0 0-1.414 0z" /><path d="M8.916 17.912a1 1 0 0 0 0 1.415l2.377 2.376a1 1 0 0 0 1.414 0l2.377-2.376a1 1 0 0 0 0-1.415l-2.377-2.376a1 1 0 0 0-1.414 0z" /><path d="M8.916 4.674a1 1 0 0 0 0 1.414l2.377 2.376a1 1 0 0 0 1.414 0l2.377-2.376a1 1 0 0 0 0-1.414l-2.377-2.377a1 1 0 0 0-1.414 0z" /></svg></Sample></div>;
}

function Keys() {
  return <div className="samples"><Sample caption="Shortcuts"><div className="kit-row"><kbd>⌘ K</kbd><kbd>⌘ ⇧ P</kbd><kbd>Esc</kbd></div></Sample><Sample caption="Outline"><kbd className="kit-kbd-outline">⌘ S</kbd></Sample></div>;
}

function Labels() {
  return <div className="samples"><Sample caption="Plain">Fixture name</Sample><Sample caption="With secondary text"><span>Fixture name <span className="kit-muted">(optional)</span></span></Sample><Sample caption="Highlighted match"><span>Component <mark>gall</mark>ery</span></Sample><Sample caption="Masked"><span aria-label="Masked value">•••••••••••••••••</span></Sample></div>;
}

function Links() {
  return <div className="samples"><Sample caption="External"><a className="kit-link" href="https://example.com/component-gallery" target="_blank" rel="noreferrer">example.com</a></Sample><Sample caption="Disabled"><span className="kit-link" role="link" aria-disabled="true">example.com</span></Sample></div>;
}

function Markers() {
  return <div className="samples">{[['Plain', 'Ada Example joined'], ['Separator', 'Earlier messages'], ['Border', 'Unread messages'], ['Loading', 'Thinking…']].map(([caption, text]) => <Sample key={caption} caption={caption}><div className={`kit-marker kit-marker-${caption.toLowerCase()}`}>{caption === 'Loading' && <Spinner />}<span>{text}</span></div></Sample>)}</div>;
}

function Message({ name, text, sent = false, delivered = false }: { name: string; text: string; sent?: boolean; delivered?: boolean }) {
  return <div className={`kit-message ${sent ? 'kit-message-sent' : ''}`}><Avatar name={name} small /><div><small>{name}</small><div className={`kit-bubble ${sent ? '' : 'kit-bubble-secondary'}`}>{text}</div>{delivered && <small className="kit-delivered">Delivered</small>}</div></div>;
}

function Messages() {
  return <div className="samples"><Sample caption="Received and sent"><div className="kit-messages"><Message name="Ada Example" text="Can you review the fixture?" /><Message name="Sam Fixture" text="Sure, after lunch." sent delivered /></div></Sample></div>;
}

const conversation = [
  ['Ada Example', 'Is the fixture ready?'],
  ['Sam Fixture', 'Almost. The gallery shows every component now.'],
  ['Ada Example', 'Does the scroller follow new messages?'],
  ['Sam Fixture', 'Yes, while you stay at the end.'],
  ['Ada Example', 'And when I scroll up?'],
  ['Sam Fixture', 'It keeps your place and shows a jump button.'],
  ['Ada Example', 'Great.'],
  ['Sam Fixture', 'Scroll up to try it.'],
];

function MessageScrollers() {
  const scroller = useRef<HTMLDivElement>(null);
  const [atEnd, setAtEnd] = useState(true);
  useEffect(() => { if (scroller.current) scroller.current.scrollTop = scroller.current.scrollHeight; }, []);
  function jump() {
    if (scroller.current) scroller.current.scrollTop = scroller.current.scrollHeight;
    setAtEnd(true);
  }
  return <div className="samples"><Sample caption="Follows the end; scroll up for the jump button"><div className="kit-message-scroller"><div ref={scroller} tabIndex={0} aria-label="Fixture conversation" className="kit-conversation" onScroll={event => { const element = event.currentTarget; setAtEnd(element.scrollHeight - element.scrollTop - element.clientHeight < 8); }}>{conversation.map(([name, text], index) => <Message key={text} name={name} text={text} sent={index % 2 === 1} />)}</div>{!atEnd && <Button variant="outline" className="kit-jump" aria-label="Jump to latest message" onClick={jump}><ChevronsDown /></Button>}</div></Sample></div>;
}

function Paginations() {
  const [page, setPage] = useState(1);
  const pages = [...new Set([1, ...[page - 1, page, page + 1].filter(value => value > 1 && value < 12), 12])];
  const arrows = (compact: boolean) => <nav className="kit-row kit-pagination" aria-label={compact ? 'Compact fixture pages' : 'Fixture pages'}><Button variant="ghost" disabled={page === 1} aria-label="Previous page" onClick={() => setPage(page - 1)}><ChevronLeft /></Button>{compact ? <span>{page} / 12</span> : pages.map((value, index) => <span key={value} className="kit-row">{index > 0 && value - pages[index - 1] > 1 && <span>…</span>}<Button variant={value === page ? 'selected' : 'ghost'} aria-current={value === page ? 'page' : undefined} aria-label={`Page ${value}`} onClick={() => setPage(value)}>{value}</Button></span>)}<Button variant="ghost" disabled={page === 12} aria-label="Next page" onClick={() => setPage(page + 1)}><ChevronRight /></Button></nav>;
  return <div className="samples"><Sample caption="Interactive">{arrows(false)}</Sample><Sample caption="Compact">{arrows(true)}</Sample></div>;
}

function Progress() {
  return <div className="samples"><Sample caption="Bar, accent"><progress className="kit-progress" value={60} max={100} aria-label="Running sample" /></Sample><Sample caption="Bar, red"><progress className="kit-progress kit-progress-danger" value={60} max={100} aria-label="Failed sample" /></Sample><Sample caption="Circle"><svg className="kit-progress-circle" viewBox="0 0 36 36" role="progressbar" aria-label="Fixture upload" aria-valuemin={0} aria-valuemax={100} aria-valuenow={60}><circle className="kit-circle-track" cx="18" cy="18" r="15.9" /><circle cx="18" cy="18" r="15.9" strokeDasharray="60 100" /></svg></Sample><Sample caption="Circle, loading"><Spinner label="Fixture loading" /></Sample></div>;
}

function Questionnaires() {
  const [step, setStep] = useState(0);
  const [answers, setAnswers] = useState(['', '']);
  const id = useId();
  const questions = [['Which theme do you use?', 'Dark', 'Light'], ['Which density do you prefer?', 'Compact', 'Comfortable']];
  return <div className="samples"><Sample caption="Two questions; choose and go next"><div className="kit-questionnaire"><progress value={step + 1} max={2} aria-label="Question progress" /><fieldset><legend>{questions[step][0]}</legend>{questions[step].slice(1).map(choice => <label className="kit-question-choice" key={choice}><input type="radio" name={`${id}-${step}`} checked={answers[step] === choice} onChange={() => setAnswers(answers.map((value, index) => index === step ? choice : value))} />{choice}</label>)}</fieldset><div className="kit-row"><Button variant="outline" disabled={step === 0} onClick={() => setStep(0)}>Previous</Button><span className="kit-muted">{step + 1} / 2</span><Button disabled={!answers[step] || step === 1} onClick={() => setStep(1)}>Next</Button></div></div></Sample></div>;
}

function Radios() {
  const [selected, setSelected] = useState('Weekly');
  const id = useId();
  return <div className="samples"><Sample caption="Group"><div className="kit-row" role="radiogroup" aria-label="Fixture frequency">{['Daily', 'Weekly', 'Monthly'].map(label => <label className="kit-check" key={label}><input type="radio" name={id} checked={selected === label} onChange={() => setSelected(label)} />{label}</label>)}</div></Sample><Sample caption="Disabled"><label className="kit-check"><input type="radio" disabled />Unavailable</label></Sample></div>;
}

function Ratings() {
  const [rating, setRating] = useState(3);
  const id = useId();
  return <div className="samples">{[false, true].map(disabled => <Sample caption={disabled ? 'Disabled' : 'Interactive'} key={String(disabled)}><div className="kit-rating" role="radiogroup" aria-label={disabled ? 'Locked fixture rating' : 'Fixture rating'}>{[1, 2, 3, 4, 5].map(value => <label key={value}><input type="radio" name={`${id}-${disabled}`} aria-label={`${value} stars`} disabled={disabled} checked={(disabled ? 3 : rating) === value} onChange={() => setRating(value)} /><Star className={value <= (disabled ? 3 : rating) ? 'kit-star-filled' : ''} /></label>)}</div></Sample>)}</div>;
}

function Separators() {
  return <div className="samples"><Sample caption="Horizontal"><hr className="kit-separator" /></Sample><Sample caption="Labelled"><div className="kit-separator-label"><hr />or<hr /></div></Sample><Sample caption="Dashed"><hr className="kit-separator kit-separator-dashed" /></Sample><Sample caption="Vertical"><div className="kit-row kit-vertical-row">Left<span role="separator" aria-orientation="vertical" className="kit-separator-vertical" />Right</div></Sample></div>;
}

function Shimmers() {
  return <div className="samples"><Sample caption="Streaming text"><span className="kit-shimmer" role="status">Thinking about the fixture…</span></Sample></div>;
}

function Skeletons() {
  return <div className="samples"><Sample caption="Row placeholder"><div className="kit-row" role="status" aria-label="Loading fixture"><span className="kit-skeleton kit-skeleton-avatar" /><div className="kit-stack"><span className="kit-skeleton kit-skeleton-line" /><span className="kit-skeleton kit-skeleton-short" /></div></div></Sample></div>;
}

function Sliders() {
  const [value, setValue] = useState(40);
  return <div className="samples"><Sample caption="Horizontal"><input className="kit-slider" type="range" min={0} max={100} step={5} value={value} aria-label="Fixture volume" aria-valuetext={String(value)} onChange={event => setValue(Number(event.target.value))} /></Sample><Sample caption="Disabled"><input className="kit-slider" type="range" min={0} max={100} step={5} value={40} disabled aria-label="Locked fixture volume" /></Sample></div>;
}

function Spinner({ label = 'Loading', size = 'md' }: { label?: string; size?: string }) {
  return <span className={`kit-spinner kit-spinner-${size}`} role="status" aria-label={label} />;
}

function Spinners() {
  return <div className="samples">{['xs', 'sm', 'md'].map((size, index) => <Sample caption={['xsmall', 'small', 'medium'][index]} key={size}><Spinner size={size} /></Sample>)}</div>;
}

function Steppers() {
  const [step, setStep] = useState(1);
  return <div className="samples"><Sample caption="Interactive; click a step"><ol className="kit-stepper">{['Details', 'Review', 'Done'].map((label, index) => <li key={label}><Button variant={step === index ? 'selected' : 'ghost'} aria-current={step === index ? 'step' : undefined} onClick={() => setStep(index)}><span className={`kit-step-number ${index <= step ? 'kit-step-active' : ''}`}>{index < step ? <Check /> : index + 1}</span>{label}</Button></li>)}</ol></Sample></div>;
}

function Switches() {
  const [on, setOn] = useState(true);
  return <div className="samples"><Sample caption="Interactive, through toggle::switch"><button className="kit-switch" role="switch" aria-label="Fixture setting" aria-checked={on} onClick={() => setOn(!on)}><span /></button></Sample><Sample caption="Disabled"><button className="kit-switch" role="switch" aria-label="Locked fixture setting" aria-checked="true" disabled><span /></button></Sample></div>;
}

function Tags() {
  return <div className="samples"><Sample caption="Variants"><div className="kit-row">{['Default', 'Primary', 'Secondary', 'Success', 'Warning', 'Danger', 'Info'].map(label => <span className={`kit-tag kit-tag-${label.toLowerCase()}`} key={label}>{label}</span>)}</div></Sample><Sample caption="Outline and small"><div className="kit-row"><span className="kit-tag kit-tag-primary kit-tag-outline">Outline</span><span className="kit-tag kit-tag-secondary kit-tag-small">Small</span><span className="kit-tag kit-tag-info kit-tag-pill">Pill</span></div></Sample></div>;
}

function TextViews() {
  return <div className="samples"><Sample caption="Markdown"><div className="kit-text-view"><p><strong>GPUI Kit</strong> <code>TextView</code> renders <em>Markdown</em>:</p><ul><li>lists</li><li><code>code</code></li><li><a className="kit-link" href="https://example.com/component-gallery" target="_blank" rel="noreferrer">links</a></li></ul></div></Sample><Sample caption="HTML"><div className="kit-text-view"><p><b>HTML</b> with <i>inline</i> <code>code</code>.</p></div></Sample></div>;
}

function Toggles() {
  const [pin, setPin] = useState(false);
  const [formats, setFormats] = useState<string[]>([]);
  return <div className="samples"><Sample caption="Interactive"><Button variant={pin ? 'selected' : 'ghost'} aria-pressed={pin} onClick={() => setPin(!pin)}><Pin />Pin</Button></Sample><Sample caption="Segmented group"><div className="kit-segments" role="group" aria-label="Text formatting">{['Bold', 'Italic', 'Underline'].map(label => <Button key={label} variant={formats.includes(label) ? 'selected' : 'outline'} aria-pressed={formats.includes(label)} onClick={() => setFormats(formats.includes(label) ? formats.filter(value => value !== label) : [...formats, label])}>{label}</Button>)}</div></Sample></div>;
}

function Tooltips() {
  const id = useId();
  const [dismissed, setDismissed] = useState(false);
  return <div className="samples"><Sample caption="Text; hover to show"><div className={`kit-tooltip-owner ${dismissed ? 'kit-tooltip-dismissed' : ''}`} onPointerEnter={() => setDismissed(false)} onFocus={() => setDismissed(false)} onKeyDown={event => { if (event.key === 'Escape') setDismissed(true); }}><Button variant="outline" aria-describedby={`${id}-text`}>Hover me</Button><span role="tooltip" id={`${id}-text`} className="kit-tooltip">A short explanation</span></div></Sample><Sample caption="Custom element; hover to show"><div className={`kit-tooltip-owner ${dismissed ? 'kit-tooltip-dismissed' : ''}`} onPointerEnter={() => setDismissed(false)} onFocus={() => setDismissed(false)} onKeyDown={event => { if (event.key === 'Escape') setDismissed(true); }}><span tabIndex={0} aria-describedby={`${id}-custom`}>Hover for details</span><span role="tooltip" id={`${id}-custom`} className="kit-tooltip"><span>Fixture details</span><small>Built with Tooltip::element</small></span></div></Sample></div>;
}

export const kitBasicSamples: Record<string, ComponentType> = {
  'gpui_kit::component::accordion::Accordion': AccordionSample,
  'gpui_kit::component::alert::Alert': Alerts,
  'gpui_kit::component::attachment::Attachment': Attachments,
  'gpui_kit::component::avatar::Avatar': Avatars,
  'gpui_kit::component::badge::Badge': Badges,
  'gpui_kit::component::breadcrumb::Breadcrumb': Breadcrumbs,
  'gpui_kit::component::bubble::Bubble': Bubbles,
  'gpui_kit::component::button::Button': Buttons,
  'gpui_kit::component::checkbox::Checkbox': Checkboxes,
  'gpui_kit::component::clipboard::Clipboard': Clipboards,
  'gpui_kit::component::collapsible::Collapsible': Collapsibles,
  'gpui_kit::component::button::DropdownButton': DropdownButtons,
  'gpui_kit::component::empty::Empty': Empties,
  'gpui_kit::component::Icon': Icons,
  'gpui_kit::img': Images,
  'gpui_kit::component::kbd::Kbd': Keys,
  'gpui_kit::component::label::Label': Labels,
  'gpui_kit::component::link::Link': Links,
  'gpui_kit::component::marker::Marker': Markers,
  'gpui_kit::component::message::Message': Messages,
  'gpui_kit::component::message_scroller::MessageScroller': MessageScrollers,
  'gpui_kit::component::pagination::Pagination': Paginations,
  'gpui_kit::component::progress::Progress': Progress,
  'gpui_kit::component::questionnaire::Questionnaire': Questionnaires,
  'gpui_kit::component::radio::Radio': Radios,
  'gpui_kit::component::rating::Rating': Ratings,
  'gpui_kit::component::separator::Separator': Separators,
  'gpui_kit::component::shimmer::ShimmerText': Shimmers,
  'gpui_kit::component::skeleton::Skeleton': Skeletons,
  'gpui_kit::component::slider::Slider': Sliders,
  'gpui_kit::component::spinner::Spinner': Spinners,
  'gpui_kit::component::stepper::Stepper': Steppers,
  'gpui_kit::component::switch::Switch': Switches,
  'gpui_kit::component::tag::Tag': Tags,
  'gpui_kit::component::text::TextView': TextViews,
  'gpui_kit::component::button::Toggle': Toggles,
  'gpui_kit::component::tooltip::Tooltip': Tooltips,
};
