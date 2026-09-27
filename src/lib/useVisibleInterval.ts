import { useEffect, useRef, type DependencyList } from 'react';
import { getIsWindowVisible, subscribeWindowVisibility } from './windowVisibility';

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

    let timerId: number | null = null;

    const startTimer = () => {
      if (timerId !== null) return;
      // Immediate tick on resume so data updates immediately without waiting intervalMs
      callbackRef.current();
      timerId = window.setInterval(() => {
        if (getIsWindowVisible() && document.visibilityState === 'visible') {
          callbackRef.current();
        }
      }, intervalMs);
    };

    const stopTimer = () => {
      if (timerId !== null) {
        window.clearInterval(timerId);
        timerId = null;
      }
    };

    if (getIsWindowVisible() && document.visibilityState === 'visible') {
      startTimer();
    }

    const unsubscribe = subscribeWindowVisibility((visible) => {
      if (visible && document.visibilityState === 'visible') {
        startTimer();
      } else {
        stopTimer();
      }
    });

    const onVisible = () => {
      if (getIsWindowVisible() && document.visibilityState === 'visible') {
        startTimer();
      } else {
        stopTimer();
      }
    };

    document.addEventListener('visibilitychange', onVisible);

    return () => {
      stopTimer();
      unsubscribe();
      document.removeEventListener('visibilitychange', onVisible);
    };
  }, [enabled, intervalMs, ...deps]);
}
