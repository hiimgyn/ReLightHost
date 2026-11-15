// Run: node scripts/check-drop-target.ts   (Node >= 23.6 strips TS types natively)
import assert from 'node:assert/strict';
import { resolveDropTarget, slotToIndex } from '../src/components/chain/dropTarget.ts';

// Cards 200 wide with 40px gaps; 3 per row, rows 160 tall with 20px gap.
const card = (i: number) => {
  const col = i % 3;
  const row = Math.floor(i / 3);
  return { left: 40 + col * 240, right: 240 + col * 240, top: row * 180, bottom: row * 180 + 160 };
};
const cards = [0, 1, 2, 3, 4].map(card); // row 0: 0,1,2  row 1: 3,4
const bounds = { left: 0, right: 760, top: -24, bottom: 364 };
const at = (x: number, y: number) => resolveDropTarget(cards, x, y, bounds);

assert.deepEqual(at(10, 80), { kind: 'insert', slot: 0 }, 'left of first card -> start');
assert.deepEqual(at(50, 80), { kind: 'insert', slot: 0 }, 'left edge of first card -> start');
assert.deepEqual(at(140, 80), { kind: 'swap', index: 0 }, 'card centre -> swap');
assert.deepEqual(at(260, 80), { kind: 'insert', slot: 1 }, 'gap -> between');
assert.deepEqual(at(750, 80), { kind: 'insert', slot: 3 }, 'right of row-0 end -> before first of row 1');
assert.deepEqual(at(700, 260), { kind: 'insert', slot: 5 }, 'right of last card -> end');
assert.deepEqual(at(470, 360), { kind: 'insert', slot: 5 }, 'below last row, right edge -> end');
assert.equal(at(100, 400), null, 'outside bounds -> cancel');
assert.equal(resolveDropTarget([], 10, 10, bounds), null, 'empty chain');

// Lifted swap target (translateY(-8px)) must not split its row.
const lifted = [card(0), { ...card(1), top: -8, bottom: 152 }, card(2)];
assert.deepEqual(resolveDropTarget(lifted, 750, 80, bounds), { kind: 'insert', slot: 3 });

// slot -> to (remove-then-insert), including off-by-one moving down.
assert.equal(slotToIndex(0, 5), 4, 'first -> end');
assert.equal(slotToIndex(4, 0), 0, 'last -> start');
assert.equal(slotToIndex(1, 3), 2, 'down by one gap');
assert.equal(slotToIndex(2, 2), null, 'own left slot = no-op');
assert.equal(slotToIndex(2, 3), null, 'own right slot = no-op');
assert.equal(slotToIndex(0, 2), 1, 'chain of 2: first -> end');
assert.equal(slotToIndex(1, 0), 0, 'chain of 2: last -> start');
assert.equal(slotToIndex(0, 1), null, 'chain of 1: only slots are no-ops');

console.log('drop-target checks passed');
