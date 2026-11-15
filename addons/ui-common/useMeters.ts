import { useRef, useState } from 'react';
import { tauri, useVisibleInterval } from './host';

const NONE: readonly number[] = [];

/** Polls the add-on's meter slots (order defined by the add-on) while `enabled`. */
export function useMeters(instanceId: string, enabled: boolean): readonly number[] {
  const [meters, setMeters] = useState<readonly number[]>(NONE);
  const inFlight = useRef(false);

  useVisibleInterval(() => {
    if (inFlight.current) return;
    inFlight.current = true;
    tauri.getAddonMeters([instanceId])
      .then((rows) => setMeters(rows.find(([id]) => id === instanceId)?.[1] ?? NONE))
      .catch(() => {})
      .finally(() => { inFlight.current = false; });
  }, 33, enabled, [instanceId]);

  return enabled ? meters : NONE;
}
