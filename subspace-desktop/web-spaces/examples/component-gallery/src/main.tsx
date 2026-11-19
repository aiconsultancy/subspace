import { StrictMode, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { cards, filterCards, readSelection, sections, themeNames, type CardId, type ThemeId } from './catalogue';
import { applyTheme } from './themes';
import { GalleryCard, implementedCount } from './components/gallery-card';
import { Button } from './components/ui/button';
import { Input } from './components/ui/input';
import { nativeViewCount, nativeImage, nativeProvenance } from './evidence';
import './styles.css';

function App({ initial }: { initial: ReturnType<typeof readSelection> }) {
  const [theme, setTheme] = useState(initial.theme);
  const [card, setCard] = useState(initial.card);
  const [query, setQuery] = useState('');
  const [section, setSection] = useState('all');
  const [failedImage, setFailedImage] = useState('');
  const filtered = filterCards(query, section);
  const selected = cards[card];
  const image = nativeImage(theme, card);
  const index = filtered.findIndex(item => item.id === card);
  function select(nextTheme: ThemeId, nextCard: CardId) {
    applyTheme(nextTheme);
    setTheme(nextTheme);
    setCard(nextCard);
    const params = new URLSearchParams({ theme: nextTheme, card: nextCard });
    history.replaceState(null, '', `?${params}`);
    document.title = `Component gallery · ${cards[nextCard].name}`;
  }
  const viewport = <div className="gallery-viewport"><GalleryCard key={card} card={card} /></div>;
  if (initial.capture) return viewport;
  return <main className="comparison-app" data-implementation-count={implementedCount}>
    <header className="app-header">
      <h1>Component gallery</h1>
      <p>{implementedCount} / {Object.keys(cards).length} native catalogue cards implemented in React · {implementedCount * 2} theme views.</p>
      <p>Owner visually accepted the first three cards in both themes. Visual review is the acceptance criterion; strict comparison remains optional and unchanged (initial six: 0/6).</p>
      <p>Local feature-branch preview. No push, hooks, CI or merge readiness is asserted.</p>
      <div className="selectors">
        <label>Theme<select aria-label="Theme" value={theme} onChange={event => select(event.target.value as ThemeId, card)}>
          {Object.entries(themeNames).map(([id, name]) => <option key={id} value={id}>{name}</option>)}
        </select></label>
        <label>Search components<Input aria-label="Search components" type="search" value={query} onChange={event => setQuery(event.target.value)} placeholder="Name, category or state" /></label>
        <label>Component<select aria-label="Component" value={card} onChange={event => select(theme, event.target.value)}>
          {Object.entries(cards).map(([id, item]) => <option key={id} value={id}>{item.name}</option>)}
        </select></label>
      </div>
    </header>
    <div className="gallery-workspace">
      <aside className="gallery-navigation" aria-label="Catalogue navigation">
        <label>Category<select aria-label="Category" value={section} onChange={event => setSection(event.target.value)}>
          <option value="all">All categories ({Object.keys(cards).length})</option>
          {sections.map(([id, title]) => <option key={id} value={id}>{title} ({Object.values(cards).filter(item => item.section === id).length})</option>)}
        </select></label>
        <p aria-live="polite">{filtered.length} matching cards</p>
        <div className="card-navigation" role="group" aria-label="Matching components">
          {filtered.map(item => <Button key={item.id} variant={card === item.id ? 'selected' : 'ghost'} aria-pressed={card === item.id} onClick={() => select(theme, item.id)}>{item.name}</Button>)}
        </div>
        {filtered.length === 0 && <p>No matching cards. Clear the search or choose another category.</p>}
      </aside>
      <div className="gallery-detail">
        <div className="review-navigation" aria-label="Review controls">
          <Button variant="outline" disabled={index <= 0} onClick={() => select(theme, filtered[index - 1].id)}>Previous card</Button>
          <p>{selected.sectionTitle} · {index >= 0 ? `${index + 1} of ${filtered.length}` : 'Selection outside filter'}</p>
          <Button variant="outline" disabled={index < 0 || index === filtered.length - 1} onClick={() => select(theme, filtered[index + 1].id)}>Next card</Button>
        </div>
        <div className="comparison-panes">
          <section aria-label="Native reference image">
            <h2>GPUI native reference <span>{nativeProvenance(theme, card)}</span></h2>
            <div className="pane-scroll">{image && failedImage !== image
              ? <img className="native-reference" width="800" height="600" src={image} onError={() => setFailedImage(image)} alt={`${themeNames[theme]} native ${selected.name}. Static reference, not the React implementation.`} />
              : <p className="missing-evidence">Native reference not published for this card. React coverage does not imply native capture coverage.</p>}
            </div>
          </section>
          <section aria-label="Interactive React examples">
            <h2>React / shadcn <span>Real interactive DOM · {themeNames[theme]}</span></h2>
            <div className="pane-scroll">{viewport}</div>
          </section>
        </div>
        <details className="coverage-details"><summary>Coverage and fixture limits</summary>
          <p>{implementedCount} React cards in each theme. {nativeViewCount} / 360 current Bowser native views published. Original six prior-host references remain historical evidence.</p>
          <p>Shell cards describe their production composition; web controls do not open the native host. Speech is unsupported in the native build. OS-native menus and window dragging cannot be reproduced by a browser. Fixture controls never start real sign-in, service calls or host operations.</p>
          <p>{selected.name}: {selected.states}</p>
        </details>
      </div>
    </div>
    <footer>Use category, search and Previous/Next to review every card. Theme changes retain the selection; changing cards resets fixture state. Native images are static; the adjacent React pane contains the actual controls.</footer>
  </main>;
}

const root = createRoot(document.getElementById('root')!);
try {
  const initial = readSelection(location.search);
  applyTheme(initial.theme);
  document.documentElement.dataset.capture = String(initial.capture);
  document.documentElement.dataset.full = String(new URLSearchParams(location.search).get('full') === '1');
  root.render(<StrictMode><App initial={initial} /></StrictMode>);
} catch (error) {
  root.render(<main role="alert"><h1>Cannot open gallery</h1><p>{error instanceof Error ? error.message : 'Invalid gallery selection.'}</p><a href="./">Reset selection</a></main>);
}
