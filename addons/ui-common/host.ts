// Single gateway from add-on UIs to host code. Add-on files import host code only from here.
export * as tauri from '../../src/lib/tauri';
export { getPluginScope } from '../../src/lib/tauri';
export type { PluginInstanceInfo } from '../../src/lib/types';
export { useTranslation } from '../../src/i18n';
export { useThemeStore } from '../../src/stores/themeStore';
export { useAudioStore } from '../../src/stores/audioStore';
export { usePluginStore } from '../../src/stores/pluginStore';
export { useWindowVisibility } from '../../src/lib/windowVisibility';
export { useVisibleInterval } from '../../src/lib/useVisibleInterval';
