// No JSX / React imports: also loaded directly by scripts/check-addon-ui.ts.
type Timers = {
  setTimeout: (fn: () => void, ms: number) => unknown;
  clearTimeout: (id: any) => void;
};

/**
 * Leading + trailing throttle per parameter id. The first change after idle is sent
 * immediately and not queued; changes inside the window are coalesced per id and the
 * latest value of each is sent when the window ends. dispose() flushes pending.
 */
export function createThrottledSender(
  send: (id: number, value: number) => void,
  ms = 40,
  timers: Timers = globalThis as unknown as Timers,
) {
  const pending = new Map<number, number>();
  let timer: unknown = null;

  const flush = () => {
    const entries = [...pending];
    pending.clear();
    entries.forEach(([id, v]) => send(id, v));
    return entries.length > 0;
  };
  const arm = () => {
    timer = timers.setTimeout(() => {
      timer = null;
      if (flush()) arm(); // keep the rate limit after a trailing send
    }, ms);
  };

  return {
    set(id: number, value: number) {
      if (timer === null) {
        send(id, value);
        arm();
      } else {
        pending.set(id, value);
      }
    },
    dispose() {
      if (timer !== null) timers.clearTimeout(timer);
      timer = null;
      flush();
    },
  };
}
