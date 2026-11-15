// Run: node scripts/check-addon-ui.ts   (Node >= 23.6 strips TS types natively)
import assert from 'node:assert/strict';
import { createThrottledSender } from '../addons/ui-common/throttle.ts';

// Fake clock: timers fire only when advance() passes their due time.
function fakeClock() {
  let now = 0;
  let nextId = 1;
  const timers = new Map<number, { at: number; fn: () => void }>();
  return {
    setTimeout: (fn: () => void, ms: number) => { const id = nextId++; timers.set(id, { at: now + ms, fn }); return id; },
    clearTimeout: (id: unknown) => { timers.delete(id as number); },
    advance(ms: number) {
      const end = now + ms;
      for (;;) {
        const due = [...timers.entries()].filter(([, t]) => t.at <= end).sort((a, b) => a[1].at - b[1].at)[0];
        if (!due) break;
        now = due[1].at;
        timers.delete(due[0]);
        due[1].fn();
      }
      now = end;
    },
  };
}

const setup = () => {
  const clock = fakeClock();
  const sent: [number, number][] = [];
  const s = createThrottledSender((id, v) => sent.push([id, v]), 40, clock);
  return { clock, sent, s };
};

{ // one change -> exactly one send, immediately, and no trailing re-send
  const { clock, sent, s } = setup();
  s.set(1, 0.5);
  assert.deepEqual(sent, [[1, 0.5]]);
  clock.advance(500);
  assert.deepEqual(sent, [[1, 0.5]]);
}
{ // trailing send carries the latest value per id
  const { clock, sent, s } = setup();
  s.set(1, 1); s.set(1, 2); s.set(1, 3); s.set(2, 7); s.set(2, 8);
  assert.deepEqual(sent, [[1, 1]]);
  clock.advance(40);
  assert.deepEqual(sent, [[1, 1], [1, 3], [2, 8]]);
  clock.advance(500);
  assert.equal(sent.length, 3);
  s.set(1, 9); // idle again -> immediate
  assert.deepEqual(sent[3], [1, 9]);
}
{ // dispose flushes pending and cancels the timer
  const { clock, sent, s } = setup();
  s.set(1, 1); s.set(1, 2);
  s.dispose();
  assert.deepEqual(sent, [[1, 1], [1, 2]]);
  clock.advance(500);
  assert.equal(sent.length, 2);
}
console.log('addon-ui checks ok');
