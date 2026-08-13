import { lazy, Suspense } from 'react';
import type { PluginInstanceInfo } from '../../lib/types';

const NoiseSuppressorGui = lazy(() => import('../plugin-gui/NoiseSuppressorGui'));
const CompressorGui = lazy(() => import('../plugin-gui/CompressorGui'));
const VoiceGui = lazy(() => import('../plugin-gui/VoiceGui'));

const BUILTIN_GUI_BY_PLUGIN_ID: Record<string, typeof NoiseSuppressorGui> = {
  'builtin::noise_suppressor': NoiseSuppressorGui,
  'builtin::compressor': CompressorGui,
  'builtin::voice': VoiceGui,
};

interface BuiltinPluginGuiSwitchProps {
  plugin: PluginInstanceInfo;
  open: boolean;
  onClose: () => void;
}

/** Renders the matching built-in GUI panel for a plugin_id, if one exists. */
export default function BuiltinPluginGuiSwitch({ plugin, open, onClose }: BuiltinPluginGuiSwitchProps) {
  if (!open || plugin.format !== 'builtin') return null;

  const Gui = BUILTIN_GUI_BY_PLUGIN_ID[plugin.plugin_id];
  if (!Gui) return null;

  return (
    <Suspense fallback={null}>
      <Gui plugin={plugin} isOpen={open} onClose={onClose} />
    </Suspense>
  );
}
