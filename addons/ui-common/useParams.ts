import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { tauri } from './host';
import type { PluginInstanceInfo } from './host';
import { createThrottledSender } from './throttle';

/**
 * Parameter state of an add-on GUI. Initial values come from `plugin.parameters`
 * (falling back to `defaults`); one authoritative `getPluginParameters` runs each time
 * the panel opens. `set` updates local state and sends through a 40 ms throttle.
 */
export function useParams(
  plugin: PluginInstanceInfo,
  isOpen: boolean,
  defaults: Record<number, number>,
) {
  const instanceId = plugin.instance_id;
  const pluginRef = useRef(plugin);
  pluginRef.current = plugin;
  const defaultsRef = useRef(defaults);

  const fromPlugin = useCallback(
    (params: { id: number; value: number }[] = pluginRef.current.parameters) =>
      Object.fromEntries(
        Object.entries(defaultsRef.current).map(([id, def]) => {
          const n = Number(id);
          return [n, params.find((p) => p.id === n)?.value
            ?? pluginRef.current.parameters.find((p) => p.id === n)?.value
            ?? def];
        }),
      ) as Record<number, number>,
    [],
  );

  const [values, setValues] = useState<Record<number, number>>(() => fromPlugin());

  const sender = useMemo(
    () => createThrottledSender((id, v) => { tauri.setPluginParameter(instanceId, id, v).catch(() => {}); }),
    [instanceId],
  );
  useEffect(() => () => sender.dispose(), [sender]);

  useEffect(() => {
    if (!isOpen) return;
    let cancelled = false;
    tauri.getPluginParameters(instanceId).then(
      (params) => {
        if (!cancelled && params && params.length > 0) setValues(fromPlugin(params));
      },
      () => { if (!cancelled) setValues(fromPlugin()); },
    );
    return () => { cancelled = true; };
  }, [isOpen, instanceId, fromPlugin]);

  const set = useCallback((id: number, v: number) => {
    setValues((prev) => ({ ...prev, [id]: v }));
    sender.set(id, v);
  }, [sender]);

  const resetAll = useCallback(() => {
    Object.entries(defaultsRef.current).forEach(([id, v]) => set(Number(id), v));
  }, [set]);

  return { values, set, resetAll };
}
