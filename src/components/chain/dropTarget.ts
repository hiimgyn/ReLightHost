// Pure geometry for chain drag & drop — no DOM/React, so scripts/check-drop-target.ts
// can run it under plain `node`.

export interface Rect {
  left: number;
  right: number;
  top: number;
  bottom: number;
}

/** insert: slot 0 = before the first card, slot n = after the last. */
export type DropTarget = { kind: 'insert'; slot: number } | { kind: 'swap'; index: number } | null;

/**
 * Pointer → drop target over flex-wrapped cards given in chain order.
 * The middle half of a card swaps with it; anywhere else on the card's row
 * (edges, gaps, left of the first card, right of the last) inserts at the
 * nearest gap. Outside `bounds` → null (cancel).
 */
export function resolveDropTarget(cards: Rect[], x: number, y: number, bounds: Rect): DropTarget {
  if (cards.length === 0) return null;
  if (x < bounds.left || x > bounds.right || y < bounds.top || y > bounds.bottom) return null;

  // Rows = runs of vertically overlapping cards (tolerates the lifted swap target).
  let rowStart = 0;
  let rowEnd = 0;
  let bestDist = Infinity;
  for (let start = 0; start < cards.length; ) {
    let end = start;
    while (end + 1 < cards.length && cards[end + 1].top < cards[end].bottom) end++;
    const dist = Math.abs(y - (cards[start].top + cards[start].bottom) / 2);
    if (dist < bestDist) {
      bestDist = dist;
      rowStart = start;
      rowEnd = end;
    }
    start = end + 1;
  }

  for (let i = rowStart; i <= rowEnd; i++) {
    const c = cards[i];
    if (x > c.right) continue;
    const quarter = (c.right - c.left) / 4;
    if (x >= c.left + quarter && x <= c.right - quarter && y >= c.top && y <= c.bottom) {
      return { kind: 'swap', index: i };
    }
    return { kind: 'insert', slot: x < c.left + quarter * 2 ? i : i + 1 };
  }
  return { kind: 'insert', slot: rowEnd + 1 };
}

/**
 * Insert slot → `to` index for remove-then-insert reorder (frontend store and
 * backend `PluginInstanceManager::reorder` both use it). Moving down shifts by
 * one because removing the item first shifts later indices. null = no-op.
 */
export function slotToIndex(from: number, slot: number): number | null {
  const to = slot > from ? slot - 1 : slot;
  return to === from ? null : to;
}
