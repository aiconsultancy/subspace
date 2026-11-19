import { useEffect, useId, useRef, useState, type ComponentType, type ReactNode } from 'react';
import { ChevronDown, Eye, EyeOff, MicOff, Minus, Plus, Search, X } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import './kit-controls.css';

function Sample({ caption, children }: { caption: string; children: ReactNode }) {
  return <div className="sample kit-sample"><div>{children}</div><div className="sample-caption">{caption}</div></div>;
}

function Inputs() {
  const input = useRef<HTMLInputElement>(null);
  const [value, setValue] = useState('');
  const [visible, setVisible] = useState(false);
  return <div className="samples">
    <Sample caption="Placeholder, clean button"><div className="kit-field-shell"><Input ref={input} aria-label="Fixture name" placeholder="Type a fixture name" value={value} onChange={event => setValue(event.target.value)} />{value && <Button variant="ghost" size="icon" aria-label="Clear fixture name" onClick={() => { setValue(''); input.current?.focus(); }}><X /></Button>}</div></Sample>
    <Sample caption="Prefix icon"><div className="kit-field-shell kit-field-prefix"><Search aria-hidden="true" /><Input aria-label="Search fixtures" placeholder="Search fixtures" /></div></Sample>
    <Sample caption="Masked"><div className="kit-field-shell"><Input aria-label="Fixture secret" type={visible ? 'text' : 'password'} defaultValue="not-a-real-secret" /><Button variant="ghost" size="icon" aria-label={visible ? 'Hide password' : 'Show password'} aria-pressed={visible} onClick={() => setVisible(!visible)}>{visible ? <EyeOff /> : <Eye />}</Button></div></Sample>
    <Sample caption="Disabled"><div className="kit-field"><Input disabled aria-label="Disabled fixture" defaultValue="Read only" /></div></Sample>
  </div>;
}

function Textareas() {
  return <div className="samples"><Sample caption="Fixed rows"><textarea className="kit-textarea kit-field" rows={3} aria-label="Fixture text" placeholder="A few lines of fixture text" /></Sample></div>;
}

const rustSource = 'fn main() {\n    let fixture = "component gallery";\n    println!("{fixture}");\n}\n';

function Editors() {
  const [source, setSource] = useState(rustSource);
  const numbers = useRef<HTMLDivElement>(null);
  return <div className="samples"><Sample caption="Rust, with line numbers"><div className="kit-editor"><div ref={numbers} className="kit-line-numbers" aria-hidden="true">{source.split('\n').map((_, index) => <div key={index}>{index + 1}</div>)}</div><textarea aria-label="Fixture Rust source" spellCheck={false} value={source} onChange={event => setSource(event.target.value)} onScroll={event => { if (numbers.current) numbers.current.scrollTop = event.currentTarget.scrollTop; }} /></div></Sample></div>;
}

const fruit = ['Apple', 'Banana', 'Cherry', 'Date', 'Elderberry'];

function Selects() {
  return <div className="samples"><Sample caption="Through anchored::select"><select className="kit-select kit-field" aria-label="Fixture fruit" defaultValue=""><option value="" disabled>Choose a fruit</option>{fruit.map(name => <option key={name}>{name}</option>)}</select></Sample></div>;
}

function FruitCombobox({ multiple = false }: { multiple?: boolean }) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState('');
  const [values, setValues] = useState<string[]>([]);
  const [active, setActive] = useState(0);
  const id = useId();
  const input = useRef<HTMLInputElement>(null);
  const root = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const matches = fruit.filter(name => name.toLowerCase().includes(query.toLowerCase()));
  useEffect(() => { if (open) input.current?.focus(); }, [open]);
  useEffect(() => {
    if (!open) return;
    const dismiss = (event: PointerEvent) => { if (event.target instanceof Node && !root.current?.contains(event.target)) setOpen(false); };
    document.addEventListener('pointerdown', dismiss);
    return () => document.removeEventListener('pointerdown', dismiss);
  }, [open]);
  function close() { setOpen(false); trigger.current?.focus(); }
  function choose(name: string) {
    setValues(multiple ? values.includes(name) ? values.filter(value => value !== name) : [...values, name] : [name]);
    if (!multiple) close();
  }
  return <div ref={root} className="kit-combobox kit-field" onBlur={event => { if (!event.currentTarget.contains(event.relatedTarget)) setOpen(false); }} onKeyDown={event => { if (event.key === 'Escape' && open) { event.preventDefault(); event.stopPropagation(); close(); } }}>
    <Button ref={trigger} variant="outline" aria-expanded={open} aria-haspopup="listbox" aria-controls={id} aria-label={multiple ? 'Choose fruit' : 'Choose a fruit'} onClick={() => { setOpen(!open); setQuery(''); setActive(0); }} onKeyDown={event => { if (event.key === 'ArrowDown') { event.preventDefault(); setOpen(true); } }}>{values.length ? values.join(', ') : multiple ? 'Choose fruit' : 'Choose a fruit'}<ChevronDown /></Button>
    {open && <div className="kit-combobox-popup"><Input ref={input} role="combobox" aria-label="Search fruit" placeholder="Search fruit…" value={query} aria-expanded={open} aria-controls={id} aria-autocomplete="list" aria-activedescendant={matches[active] ? `${id}-${matches[active]}` : undefined} onChange={event => { setQuery(event.target.value); setActive(0); }} onKeyDown={event => {
      if (['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) {
        event.preventDefault();
        setActive(event.key === 'Home' ? 0 : event.key === 'End' ? Math.max(0, matches.length - 1) : Math.max(0, Math.min(matches.length - 1, active + (event.key === 'ArrowDown' ? 1 : -1))));
      } else if (event.key === 'Enter' && matches[active]) { event.preventDefault(); choose(matches[active]); }
    }} /><div id={id} role="listbox" aria-label="Fruit choices" aria-multiselectable={multiple || undefined}>{matches.map((name, index) => <div key={name} id={`${id}-${name}`} role="option" aria-label={name} aria-selected={values.includes(name)} className={`kit-option ${index === active ? 'kit-option-active' : ''}`} onPointerMove={() => setActive(index)} onMouseDown={event => event.preventDefault()} onClick={() => choose(name)}><span aria-hidden="true">{values.includes(name) ? '✓' : ''}</span>{name}</div>)}</div>{!matches.length && <p className="kit-muted" role="status">No fruit found</p>}</div>}
  </div>;
}

function Comboboxes() {
  return <div className="samples"><Sample caption="Searchable"><FruitCombobox /></Sample><Sample caption="Multiple"><FruitCombobox multiple /></Sample></div>;
}

function NumberInputs() {
  const [value, setValue] = useState('3');
  function step(amount: number) { setValue(String((Number(value) || 0) + amount)); }
  return <div className="samples"><Sample caption="Step buttons"><div className="kit-number kit-field"><Input type="number" aria-label="Fixture number" value={value} onChange={event => setValue(event.target.value)} /><div><Button variant="ghost" aria-label="Increase fixture number" onClick={() => step(1)}><Plus /></Button><Button variant="ghost" aria-label="Decrease fixture number" onClick={() => step(-1)}><Minus /></Button></div></div></Sample></div>;
}

function DatePickers() {
  const input = useRef<HTMLInputElement>(null);
  const [date, setDate] = useState('');
  return <div className="samples"><Sample caption="Click to open the calendar"><div className="kit-field-shell"><Input ref={input} type="date" aria-label="Choose a date" value={date} onChange={event => setDate(event.target.value)} />{date && <Button size="icon" variant="ghost" aria-label="Clear date" onClick={() => { setDate(''); input.current?.focus(); }}><X /></Button>}</div></Sample></div>;
}

function TimeFields() {
  return <div className="samples"><Sample caption="Hours and minutes; type or use arrow keys"><Input className="kit-time" type="time" step={60} aria-label="Fixture time" /></Sample></div>;
}

function OtpInputs() {
  const [digits, setDigits] = useState<string[]>(Array(6).fill(''));
  const refs = useRef<(HTMLInputElement | null)[]>([]);
  function fill(value: string, start: number) {
    const incoming = value.replace(/\D/g, '').slice(0, 6 - start).split('');
    if (!incoming.length) return;
    setDigits(current => current.map((digit, index) => index >= start && index < start + incoming.length ? incoming[index - start] : digit));
    refs.current[Math.min(5, start + incoming.length)]?.focus();
  }
  return <div className="samples"><Sample caption="Six digits, masked"><div className="kit-otp" role="group" aria-label="Six digit fixture code" onPaste={event => { event.preventDefault(); const at = refs.current.findIndex(input => input === document.activeElement); fill(event.clipboardData.getData('text'), Math.max(0, at)); }}>{digits.map((digit, index) => <Input key={index} ref={element => { refs.current[index] = element; }} type="password" inputMode="numeric" autoComplete={index === 0 ? 'one-time-code' : 'off'} pattern="[0-9]" maxLength={1} aria-label={`Digit ${index + 1} of 6`} value={digit} onFocus={event => event.target.select()} onChange={event => { if (!event.target.value) setDigits(current => current.map((value, at) => at === index ? '' : value)); else fill(event.target.value, index); }} onKeyDown={event => {
    if (event.key === 'Backspace' && !digit && index > 0) { event.preventDefault(); refs.current[index - 1]?.focus(); setDigits(current => current.map((value, at) => at === index - 1 ? '' : value)); }
    if (event.key === 'ArrowLeft' && index > 0) { event.preventDefault(); refs.current[index - 1]?.focus(); }
    if (event.key === 'ArrowRight' && index < 5) { event.preventDefault(); refs.current[index + 1]?.focus(); }
  }} />)}</div></Sample></div>;
}

function SpeechButtons() {
  return <div className="samples"><Sample caption="Unsupported in this build"><Button variant="outline" disabled title="Speech is not supported in this build" aria-label="Speech is not supported in this build"><MicOff />Speech unavailable</Button></Sample></div>;
}

function ColorPickers() {
  const [value, setValue] = useState<string | null>(null);
  const [primary, setPrimary] = useState(() => getComputedStyle(document.documentElement).getPropertyValue('--primary-background').trim().slice(0, 7));
  useEffect(() => {
    const update = () => setPrimary(getComputedStyle(document.documentElement).getPropertyValue('--primary-background').trim().slice(0, 7));
    update();
    const observer = new MutationObserver(update);
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ['style', 'data-theme'] });
    return () => observer.disconnect();
  }, []);
  return <div className="samples"><Sample caption="Click the swatch"><input className="kit-color" type="color" aria-label="Fixture colour" value={value ?? primary} onChange={event => setValue(event.target.value)} /></Sample></div>;
}

function Forms() {
  const id = useId();
  return <div className="samples"><Sample caption="Vertical"><form className="kit-form" onSubmit={event => event.preventDefault()}><label htmlFor={`${id}-name`}>Name</label><Input id={`${id}-name`} placeholder="Ada Example" /><label htmlFor={`${id}-email`}>Email <span aria-hidden="true" className="kit-tone-danger">*</span></label><Input id={`${id}-email`} type="email" required placeholder="ada@example.com" aria-describedby={`${id}-help`} /><small id={`${id}-help`} className="kit-muted">We never send mail to fixtures.</small></form></Sample><Sample caption="Horizontal"><form className="kit-form kit-form-horizontal" onSubmit={event => event.preventDefault()}><label htmlFor={`${id}-bio`}>Bio</label><textarea className="kit-textarea" id={`${id}-bio`} rows={3} placeholder="A short fixture biography" /></form></Sample></div>;
}

function InputGroups() {
  return <div className="samples"><Sample caption="Text addons"><div className="kit-input-group"><span>https://</span><Input aria-label="Fixture domain" placeholder="example" /><span>.com</span></div></Sample></div>;
}

export const kitFormSamples: Record<string, ComponentType> = {
  'gpui_kit::component::input::Input': Inputs,
  'gpui_kit::component::input::Textarea': Textareas,
  'gpui_kit::component::input::Editor': Editors,
  'gpui_kit::component::select::Select': Selects,
  'gpui_kit::component::combobox::Combobox': Comboboxes,
  'gpui_kit::component::input::NumberInput': NumberInputs,
  'gpui_kit::component::date_picker::DatePicker': DatePickers,
  'gpui_kit::component::time_field::TimeField': TimeFields,
  'gpui_kit::component::input::OtpInput': OtpInputs,
  'gpui_kit::component::speech::SpeechButton': SpeechButtons,
  'gpui_kit::component::color_picker::ColorPicker': ColorPickers,
  'gpui_kit::component::form::v_form': Forms,
  'gpui_kit::component::input::InputGroup': InputGroups,
};
