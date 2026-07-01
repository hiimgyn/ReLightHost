import { useEffect, useRef, type DependencyList } from 'react';

export function useVisibleInterval(
  callback: () => void,
  intervalMs: number,
  enabled = true,
  deps: DependencyList = [],
) {
  const callbackRef = useRef(callback);

  useEffect(() => {
    callbackRef.current = callback;
  }, [callback]);

  useEffect(() => {
    if (!enabled) {
      return;
    }

    const tick = () => {
      if (document.visibilityState === 'visible') {
        callbackRef.current();
      }
    };

    tick();
    const timerId = window.setInterval(tick, intervalMs);

    const onVisible = () => {
      if (document.visibilityState === 'visible') {
        tick();
      }
    };

    document.addEventListener('visibilitychange', onVisible);

    return () => {
      window.clearInterval(timerId);
      document.removeEventListener('visibilitychange', onVisible);
    };
  }, [enabled, intervalMs, ...deps]);
}
