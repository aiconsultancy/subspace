import inventory from './inventory.json';

const originalCards = {
  'action-button': {
    name: 'action::action_button',
    source: 'board-kit/dashboard/src/action.rs',
    usedIn: 'Labelled actions in every board and the host.',
  },
  input: {
    name: 'fields::input',
    source: 'board-kit/dashboard/src/fields.rs',
    usedIn: 'Native inputs sized by role in Board Lab, Web and the host.',
  },
  'error-box': {
    name: 'feedback::error_box',
    source: 'board-kit/dashboard/src/feedback.rs',
    usedIn: 'Board-wide load failures in GitHub diff and Web.',
  },
} as const;

export type CardId = string;
export interface Card {
  id: CardId;
  name: string;
  section: string;
  sectionTitle: string;
  library: string;
  source: string;
  states: string;
  usage: string;
  usedIn: string;
}

export const cards: Record<CardId, Card> = Object.fromEntries(inventory.map(item => {
  const original = originalCards[item.id as keyof typeof originalCards];
  return [item.id, { ...item, source: original?.source ?? item.source, usedIn: original?.usedIn ?? item.states }];
}));

export const sections = [...new Map(inventory.map(item => [item.section, item.sectionTitle])).entries()];

export function filterCards(query: string, section: string) {
  const words = query.toLowerCase().trim().split(/\s+/).filter(Boolean);
  return Object.values(cards).filter(item => (section === 'all' || item.section === section)
    && words.every(word => `${item.name} ${item.source} ${item.states} ${item.sectionTitle}`.toLowerCase().includes(word)));
}
export type ThemeId = 'one-dark' | 'cupertino-dark';

export const themeNames: Record<ThemeId, string> = {
  'one-dark': 'Subspace One Dark',
  'cupertino-dark': 'Cupertino Dark',
};

export function readSelection(search: string) {
  const params = new URLSearchParams(search);
  const card = params.get('card') ?? 'action-button';
  const theme = params.get('theme') ?? 'one-dark';
  if (!Object.hasOwn(cards, card)) throw new Error(`Unknown component: ${card}`);
  if (!Object.hasOwn(themeNames, theme)) throw new Error(`Unknown theme: ${theme}`);
  return { card: card as CardId, theme: theme as ThemeId, capture: params.get('capture') === '1' };
}
