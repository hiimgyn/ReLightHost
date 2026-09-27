import { useState, useEffect } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { listen } from '@tauri-apps/api/event';

type VisibilityListener = (visible: boolean) => void;

let isWindowVisible = true;
const listeners = new Set<VisibilityListener>();
let isInitialized = false;

export function getIsWindowVisible(): boolean {
  return isWindowVisible;
}

export function setWindowVisible(visible: boolean) {
  if (isWindowVisible === visible) return;
  isWindowVisible = visible;

  if (typeof document !== 'undefined') {
    if (visible) {
      document.documentElement.classList.remove('rh-app-hidden');
    } else {
      document.documentElement.classList.add('rh-app-hidden');
    }
  }

  listeners.forEach((fn) => {
    try {
      fn(visible);
    } catch (e) {
      console.error('Error in window visibility listener:', e);
    }
  });
}

export function subscribeWindowVisibility(listener: VisibilityListener): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function useWindowVisibility(): boolean {
  const [visible, setVisible] = useState(isWindowVisible);

  useEffect(() => {
    setVisible(isWindowVisible);
    return subscribeWindowVisibility((v) => setVisible(v));
  }, []);

  return visible;
}

export function initWindowVisibilitySync(): () => void {
  if (isInitialized) return () => {};
  isInitialized = true;

  let isMounted = true;
  let unlistenTauri: (() => void) | null = null;
  let unlistenResize: (() => void) | null = null;

  try {
    const appWindow = getCurrentWindow();

    // 1. Initial check on mount
    void Promise.all([appWindow.isVisible(), appWindow.isMinimized()])
      .then(([isVis, isMin]) => {
        if (!isMounted) return;
        setWindowVisible(isVis && !isMin);
      })
      .catch(() => {
        if (typeof document !== 'undefined') {
          setWindowVisible(document.visibilityState === 'visible');
        }
      });

    // 2. Listen to Tauri backend event 'rh:window-visibility'
    void listen<boolean>('rh:window-visibility', (event) => {
      if (!isMounted) return;
      setWindowVisible(Boolean(event.payload));
    }).then((unlisten) => {
      unlistenTauri = unlisten;
    });

    // 3. Listen to window resized (minimizing on Win32 sets width=0, height=0)
    void appWindow
      .onResized(async ({ payload: size }) => {
        if (!isMounted) return;
        try {
          const isMin =
            (size.width === 0 && size.height === 0) || (await appWindow.isMinimized());
          const isVis = await appWindow.isVisible();
          setWindowVisible(isVis && !isMin);
        } catch {
          // ignore
        }
      })
      .then((unlisten) => {
        unlistenResize = unlisten;
      });
  } catch (err) {
    console.warn('Failed to bind Tauri window events:', err);
  }

  // 4. HTML5 Page Visibility API fallback
  const onVisibilityChange = () => {
    if (document.visibilityState === 'hidden') {
      setWindowVisible(false);
    }
  };
  document.addEventListener('visibilitychange', onVisibilityChange);

  return () => {
    isMounted = false;
    isInitialized = false;
    if (unlistenTauri) unlistenTauri();
    if (unlistenResize) unlistenResize();
    document.removeEventListener('visibilitychange', onVisibilityChange);
  };
}
