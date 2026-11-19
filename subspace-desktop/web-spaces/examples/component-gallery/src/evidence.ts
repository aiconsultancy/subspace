import type { CardId, ThemeId } from './catalogue';
import coverage from './native-coverage.json';

// Published native capture coverage, not inferred from the React inventory.
// Expanded only after real native files have been captured and verified.
export const nativeCardIds: readonly CardId[] = [...new Set(coverage.map(record => record.card))];
export const nativeViewCount = coverage.length;

export function nativeProvenance(theme: ThemeId, card: CardId) {
  const record = coverage.find(record => record.theme === theme && record.card === card);
  return record ? `Current Bowser AMD native GPUI · visual-review v1 · ${record.width} × ${record.height}`
    : ['action-button', 'input', 'error-box'].includes(card)
      ? 'Historical committed prior-host reference only; new native view pending'
      : 'Current Bowser native view pending';
}

export function nativeImage(theme: ThemeId, card: CardId) {
  return coverage.find(record => record.theme === theme && record.card === card)?.url
    ?? (['action-button', 'input', 'error-box'].includes(card) ? `./references/${theme}-${card}.png` : null);
}
