import { lazy, Suspense, useMemo, useState, type ReactNode, type ComponentType } from 'react';
import { Check, Copy, Eye, EyeOff, Play, RotateCw, Save, TriangleAlert, X } from 'lucide-react';
import { cards, type CardId } from '@/catalogue';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';

function Sample({ caption, children, className = '' }: { caption: string; children: ReactNode; className?: string }) {
  return <div className={`sample ${className}`}><div>{children}</div><div className="sample-caption">{caption}</div></div>;
}

function ActionButtons() {
  return <div className="samples">
    <Sample caption="Accept"><Button variant="success"><Check />Accept</Button></Sample>
    <Sample caption="Reject"><Button variant="destructive"><X />Reject</Button></Sample>
    <Sample caption="Caution"><Button variant="warning"><TriangleAlert />Caution</Button></Sample>
    <Sample caption="Primary"><Button><Play />Primary</Button></Sample>
    <Sample caption="Neutral"><Button variant="outline"><Copy />Neutral</Button></Sample>
    <Sample caption="Quiet"><Button variant="ghost"><Eye />Quiet</Button></Sample>
    <Sample caption="No icon"><Button variant="outline">No icon</Button></Sample>
    <Sample caption="Disabled"><Button disabled><Save />Save</Button></Sample>
    <Sample caption="Selected"><Button variant="selected"><Eye />Viewed</Button></Sample>
    <Sample caption="Selected and toggled"><Button variant="selected" aria-pressed="true"><Eye />Viewed</Button></Sample>
  </div>;
}

function InputSamples() {
  const [visible, setVisible] = useState(false);
  return <div className="samples">
    <Sample caption="Search" className="input-sample"><Input aria-label="Search sample" placeholder="Fixture text" defaultValue="Search the board" /></Sample>
    <Sample caption="Configuration" className="input-sample"><Input aria-label="Configuration sample" placeholder="Fixture text" defaultValue="https://example.com/component-gallery" /></Sample>
    <Sample caption="Compact" className="input-sample"><Input aria-label="Compact sample" className="input-compact" placeholder="Fixture text" defaultValue="Compact" /></Sample>
    <Sample caption="Disabled" className="input-sample"><Input aria-label="Disabled sample" disabled defaultValue="Read only" /></Sample>
    <Sample caption="Masked" className="input-sample"><div className="masked-input">
      <Input aria-label="Masked sample" type={visible ? 'text' : 'password'} defaultValue="not-a-real-secret" />
      <Button variant="ghost" size="icon" className="mask-toggle" aria-label={visible ? 'Hide password' : 'Show password'} aria-pressed={visible} onClick={() => setVisible(!visible)}>
        {visible ? <EyeOff /> : <Eye />}
      </Button>
    </div></Sample>
  </div>;
}

function ErrorBox() {
  // Native fixture's Retry handler is ignore_click: no fake request or success.
  return <div className="samples"><Sample caption="Error with Retry">
    <div className="bounded-state"><Alert variant="destructive" className="error-state">
      <AlertTitle>Could not load</AlertTitle>
      <AlertDescription>The fixture service answered 503 Service Unavailable.</AlertDescription>
      <div className="recovery-actions"><Button><RotateCw />Retry</Button></div>
    </Alert></div>
  </Sample></div>;
}

const samples = { 'action-button': ActionButtons, input: InputSamples, 'error-box': ErrorBox };

const loadedSamples: Record<string, ComponentType> = {};

function loadSamples(section: string): Promise<Record<string, ComponentType>> {
  if (section === 'kit-basic') return import('./kit-basic').then(module => module.kitBasicSamples);
  if (section === 'kit-form') return import('./kit-form').then(module => module.kitFormSamples);
  if (section === 'kit-layout') return import('./kit-layout').then(module => module.kitLayoutSamples);
  if (section === 'kit-advanced') return import('./kit-advanced').then(module => module.kitAdvancedSamples);
  if (['buttons', 'choices', 'inputs', 'menus', 'dialogs', 'feedback'].includes(section)) {
    return import('./subspace-controls').then(module => module.subspaceControlSamples);
  }
  return import('./subspace-content').then(module => module.subspaceContentSamples);
}

const extendedSamples = Object.fromEntries(Object.values(cards).map(item => [item.name, lazy(async () => {
  const module = await loadSamples(item.section);
  Object.assign(loadedSamples, module);
  const component = module[item.name];
  if (!component) throw new Error(`No React implementation for ${item.name}`);
  return { default: component };
})]));

export const registeredSamples: Record<string, ComponentType> = {
  ...extendedSamples,
  'action::action_button': ActionButtons, 'fields::input': InputSamples,
  'feedback::error_box': ErrorBox,
};

export const implementedCount = Object.values(cards).filter(item => Object.hasOwn(registeredSamples, item.name)).length;

export function GalleryCard({ card }: { card: CardId }) {
  const [copyStatus, setCopyStatus] = useState('');
  const item = cards[card];
  // Keep the selected component identity stable while its fixture state changes.
  const Samples = useMemo(() => samples[card as keyof typeof samples] ?? loadedSamples[item.name] ?? registeredSamples[item.name], [card, item.name]);
  async function copyName() {
    try {
      await navigator.clipboard.writeText(item.name);
      setCopyStatus(`Copied ${item.name}`);
    } catch {
      setCopyStatus('Clipboard unavailable. Select and copy the component name.');
    }
  }
  if (!Samples) throw new Error(`No React implementation for ${item.name}`);
  return <Suspense fallback={<p role="status">Loading component…</p>}><article className="gallery-card" data-testid="gallery-card" data-card-id={card} data-native-name={item.name} data-implemented="true" aria-label={item.name}>
    <div className="card-title"><h2>{item.name}</h2>
      <Button variant="ghost" size="icon" aria-label={`Copy ${item.name}`} title="Copy name" onClick={copyName}><Copy /></Button>
    </div>
    <p className="card-source">{item.source}</p>
    <p className="card-usage">{item.usedIn}</p>
    <Samples />
    <p className="sr-only" role="status">{copyStatus}</p>
  </article></Suspense>;
}
