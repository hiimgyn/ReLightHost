import { lazy, type ComponentType, type LazyExoticComponent } from 'react';
import type { PluginInstanceInfo } from './ui-common/host';

export interface AddonGuiProps {
  plugin: PluginInstanceInfo;
  isOpen: boolean;
  onClose: () => void;
}

export const ADDON_GUIS: Record<string, LazyExoticComponent<ComponentType<AddonGuiProps>>> = {
  'builtin::noise_suppressor': lazy(() => import('./noise-suppressor/ui/NoiseSuppressorGui')),
  'builtin::deep_filter': lazy(() => import('./deep-filter/ui/DeepFilterNetGui')),
  'builtin::compressor': lazy(() => import('./compressor/ui/CompressorGui')),
  'builtin::voice': lazy(() => import('./voice/ui/VoiceGui')),
};
