import { useEffect, useRef, useState } from 'react';

interface UsePluginLaunchOptions {
  instanceId: string;
  pluginName: string;
  guiOpen: boolean;
  interactionLocked: boolean;
  onLaunch?: (instanceId: string) => Promise<void> | void;
}

/**
 * GUI launch state machine — shows a spinner while waiting for the plugin's
 * editor window to open, with an 8s safety fallback in case `gui_open` never
 * flips true (e.g. the plugin failed to open a window).
 */
export function usePluginLaunch({ instanceId, pluginName, guiOpen, interactionLocked, onLaunch }: UsePluginLaunchOptions) {
  const [isLaunching, setIsLaunching] = useState(false);
  const launchTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // When gui_open becomes true, clear the launching spinner
  useEffect(() => {
    if (guiOpen && isLaunching) {
      setIsLaunching(false);
      if (launchTimerRef.current) clearTimeout(launchTimerRef.current);
    }
  }, [guiOpen, isLaunching]);

  useEffect(() => () => {
    if (launchTimerRef.current) clearTimeout(launchTimerRef.current);
  }, []);

  const handleLaunch = async () => {
    if (interactionLocked) return;
    if (isLaunching) return;
    if (guiOpen) return; // already open — do nothing
    setIsLaunching(true);
    // Safety fallback: clear spinner after 8 s if gui_open never becomes true
    launchTimerRef.current = setTimeout(() => setIsLaunching(false), 8000);
    try {
      console.debug('PluginCard: launch clicked', { instanceId, name: pluginName });
      await onLaunch?.(instanceId);
    } catch {
      setIsLaunching(false);
      if (launchTimerRef.current) clearTimeout(launchTimerRef.current);
    }
  };

  return { isLaunching, handleLaunch };
}
