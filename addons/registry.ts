import { lazy, type ComponentType, type LazyExoticComponent } from 'react';
import type { PluginInstanceInfo } from './ui-common/host';
import compressor from './compressor/ui/i18n';
import voice from './voice/ui/i18n';
import noise from './noise-suppressor/ui/i18n';
import deepFilter from './deep-filter/ui/i18n';

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

const all = [noise, deepFilter, compressor, voice];

export const ADDON_STRINGS = {
  en: Object.assign({}, ...all.map((a) => a.en)) as typeof compressor.en & typeof voice.en & typeof noise.en & typeof deepFilter.en,
  vi: Object.assign({}, ...all.map((a) => a.vi)) as typeof compressor.vi & typeof voice.vi & typeof noise.vi & typeof deepFilter.vi,
};
