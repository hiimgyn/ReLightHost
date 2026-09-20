import { create } from 'zustand';
import type { PluginInfo, PluginInstanceInfo, PluginStatus } from '../lib/types';
import * as tauri from '../lib/tauri';

let scanPluginsInFlight: Promise<void> | null = null;

function crashStatusMapsEqual(
  a: Record<string, PluginStatus>,
  b: Record<string, PluginStatus>,
): boolean {
  const aKeys = Object.keys(a);
  const bKeys = Object.keys(b);
  if (aKeys.length !== bKeys.length) return false;
  for (const key of aKeys) {
    if (!(key in b)) return false;
    if (JSON.stringify(a[key]) !== JSON.stringify(b[key])) return false;
  }
  return true;
}

interface PluginStore {
  // State
  availablePlugins: PluginInfo[];
  pluginChain: PluginInstanceInfo[];
  crashStatusByInstanceId: Record<string, PluginStatus>;
  isScanning: boolean;
  isChainInitializing: boolean;
  restoreTargetCount: number | null;
  hasFetchedChainOnce: boolean;
  mutationCount: number;
  isMutating: boolean;
  
  // Actions
  scanPlugins: () => Promise<void>;
  addToChain: (plugin: PluginInfo) => Promise<void>;
  removeFromChain: (instanceId: string) => Promise<void>;
  reloadPlugin: (instanceId: string) => Promise<void>;
  toggleBypass: (instanceId: string) => Promise<void>;
  reorderChain: (fromIndex: number, toIndex: number) => Promise<void>;
  swapChain: (firstIndex: number, secondIndex: number) => Promise<void>;
  fetchChain: () => Promise<void>;
  fetchCrashStatuses: () => Promise<void>;
  setRestoreTargetCount: (count: number | null) => void;
}

export const usePluginStore = create<PluginStore>((set, get) => ({
  availablePlugins: [],
  pluginChain: [],
  crashStatusByInstanceId: {},
  isScanning: false,
  isChainInitializing: true,
  restoreTargetCount: null,
  hasFetchedChainOnce: false,
  mutationCount: 0,
  isMutating: false,

  scanPlugins: async () => {
    if (scanPluginsInFlight) {
      return scanPluginsInFlight;
    }

    const task = (async () => {
      if (get().isScanning) {
        return;
      }

      set({ isScanning: true });
      try {
        const plugins = await tauri.scanPlugins();
        set({ availablePlugins: plugins });
      } catch (error) {
        console.error('Failed to scan plugins:', error);
      } finally {
        set({ isScanning: false });
      }
    })();

    scanPluginsInFlight = task.finally(() => {
      scanPluginsInFlight = null;
    });

    return scanPluginsInFlight;
  },

  addToChain: async (plugin: PluginInfo) => {
    set((state) => {
      const next = state.mutationCount + 1;
      return { mutationCount: next, isMutating: next > 0 };
    });
    try {
      await tauri.loadPlugin(plugin);
      await get().fetchChain();
    } catch (error) {
      console.error('Failed to add plugin to chain:', error);
      throw error;
    } finally {
      set((state) => {
        const next = Math.max(0, state.mutationCount - 1);
        return { mutationCount: next, isMutating: next > 0 };
      });
    }
  },

  removeFromChain: async (instanceId: string) => {
    set((state) => {
      const next = state.mutationCount + 1;
      return { mutationCount: next, isMutating: next > 0 };
    });
    try {
      await tauri.removePlugin(instanceId);
      await get().fetchChain();
    } catch (error) {
      console.error('Failed to remove plugin from chain:', error);
      throw error;
    } finally {
      set((state) => {
        const next = Math.max(0, state.mutationCount - 1);
        return { mutationCount: next, isMutating: next > 0 };
      });
    }
  },

  // Remove + re-add a plugin at its original position so a change that only
  // takes effect at load time (e.g. VST3 sandbox forcing) applies to an
  // already-running instance, without a dedicated backend "reload" command.
  //
  // Deliberately bypasses removeFromChain/addToChain/reorderChain's own
  // fetchChain() calls and does a single fetchChain() at the very end —
  // each of those would otherwise commit its own intermediate pluginChain
  // snapshot (card vanishes, reappears at the end of the list, then jumps
  // back to its original position), which is what actually looked like a
  // freeze-then-jump. The card in this slot stays mounted with its old data
  // the whole time — the caller shows a busy state on it — and only swaps
  // to the reloaded plugin once its final position is already correct.
  reloadPlugin: async (instanceId: string) => {
    const current = get().pluginChain;
    const index = current.findIndex((p) => p.instance_id === instanceId);
    const plugin = current[index];
    if (!plugin) return;

    const pluginInfo: PluginInfo = {
      id: plugin.plugin_id,
      name: plugin.name,
      manufacture: plugin.manufacture,
      version: plugin.version,
      path: plugin.path,
      format: plugin.format,
      category: plugin.category,
    };

    set((state) => {
      const next = state.mutationCount + 1;
      return { mutationCount: next, isMutating: next > 0 };
    });
    try {
      await tauri.removePlugin(instanceId);
      await tauri.loadPlugin(pluginInfo);
      const chainAfterAdd = await tauri.getPluginChain();
      const newIndex = chainAfterAdd.length - 1;
      if (newIndex > index) {
        await tauri.reorderPluginChain(newIndex, index);
      }
      await get().fetchChain();
    } finally {
      set((state) => {
        const next = Math.max(0, state.mutationCount - 1);
        return { mutationCount: next, isMutating: next > 0 };
      });
    }
  },

  toggleBypass: async (instanceId: string) => {
    const instance = get().pluginChain.find(p => p.instance_id === instanceId);
    if (!instance) return;
    try {
      await tauri.setPluginBypass(instanceId, !instance.bypassed);
      await get().fetchChain();
    } catch (error) {
      console.error('Failed to toggle bypass:', error);
      throw error;
    }
  },

  reorderChain: async (fromIndex: number, toIndex: number) => {
    const current = get().pluginChain;
    const len = current.length;
    if (fromIndex < 0 || toIndex < 0 || fromIndex >= len || toIndex >= len || fromIndex === toIndex) {
      return;
    }

    const next = [...current];
    const [item] = next.splice(fromIndex, 1);
    if (!item) return;
    next.splice(toIndex, 0, item);

    // Optimistic UI reorder for smooth drag/drop feel.
    set({ pluginChain: next });

    set((state) => {
      const count = state.mutationCount + 1;
      return { mutationCount: count, isMutating: count > 0 };
    });

    try {
      await tauri.reorderPluginChain(fromIndex, toIndex);
    } catch (error) {
      // Revert immediately on failure, then resync with backend snapshot.
      set({ pluginChain: current });
      await get().fetchChain();
      throw error;
    } finally {
      set((state) => {
        const count = Math.max(0, state.mutationCount - 1);
        return { mutationCount: count, isMutating: count > 0 };
      });
    }
  },

  swapChain: async (firstIndex: number, secondIndex: number) => {
    const current = get().pluginChain;
    const len = current.length;
    if (
      firstIndex < 0 ||
      secondIndex < 0 ||
      firstIndex >= len ||
      secondIndex >= len ||
      firstIndex === secondIndex
    ) {
      return;
    }

    const next = [...current];
    const temp = next[firstIndex];
    next[firstIndex] = next[secondIndex];
    next[secondIndex] = temp;

    set({ pluginChain: next });

    set((state) => {
      const count = state.mutationCount + 1;
      return { mutationCount: count, isMutating: count > 0 };
    });

    try {
      await tauri.swapPluginChain(firstIndex, secondIndex);
    } catch (error) {
      set({ pluginChain: current });
      await get().fetchChain();
      throw error;
    } finally {
      set((state) => {
        const count = Math.max(0, state.mutationCount - 1);
        return { mutationCount: count, isMutating: count > 0 };
      });
    }
  },

  fetchChain: async () => {
    try {
      const chain = await tauri.getPluginChain();
      set(() => ({
        pluginChain: chain,
        isChainInitializing: false,
        hasFetchedChainOnce: true,
      }));
    } catch (error) {
      console.error('Failed to fetch plugin chain:', error);
      // Do not keep the UI permanently locked when initial fetch fails.
      set(() => ({
        isChainInitializing: false,
        hasFetchedChainOnce: true,
      }));
    }
  },

  fetchCrashStatuses: async () => {
    try {
      const statuses = await tauri.getPluginCrashStatuses();
      const next: Record<string, PluginStatus> = {};
      for (const item of statuses) {
        next[item.instance_id] = item.status;
      }
      // Avoid triggering a re-render across every subscriber when nothing changed
      // (this runs on a 10s poll regardless of whether any status actually differs).
      const prev = get().crashStatusByInstanceId;
      if (!crashStatusMapsEqual(prev, next)) {
        set({ crashStatusByInstanceId: next });
      }
    } catch (error) {
      console.error('Failed to fetch plugin crash statuses:', error);
    }
  },

  setRestoreTargetCount: (count: number | null) => {
    set({ restoreTargetCount: count });
  },

}));
