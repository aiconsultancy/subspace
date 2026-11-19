export const themes = ['one-dark', 'cupertino-dark'];
export const cards = ['action-button', 'input', 'error-box'];
export const cases = themes.flatMap(theme => cards.map(card => ({
  theme, card, id: `${theme}-${card}`,
})));

export function option(name, fallback) {
  const index = process.argv.indexOf(name);
  if (index === -1) return fallback;
  const value = process.argv[index + 1];
  if (!value || value.startsWith('--')) throw new Error(`Missing value after ${name}`);
  return value;
}
