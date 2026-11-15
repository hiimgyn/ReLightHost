import { Suspense, useEffect, useState } from 'react';
import type { PluginInstanceInfo } from '../../lib/types';
import { ADDON_GUIS } from '../../../addons/registry';

interface BuiltinPluginGuiSwitchProps {
  plugin: PluginInstanceInfo;
  open: boolean;
  onClose: () => void;
}

/** Renders the matching built-in GUI panel for a plugin_id, preserving exit transitions. */
export default function BuiltinPluginGuiSwitch({ plugin, open, onClose }: BuiltinPluginGuiSwitchProps) {
  const [hasEverOpened, setHasEverOpened] = useState(false);

  useEffect(() => {
    if (open) setHasEverOpened(true);
  }, [open]);

  if (plugin.format !== 'builtin') return null;
  if (!open && !hasEverOpened) return null;

  const Gui = ADDON_GUIS[plugin.plugin_id];
  if (!Gui) return null;

  return (
    <Suspense fallback={null}>
      <Gui plugin={plugin} isOpen={open} onClose={onClose} />
    </Suspense>
  );
}
