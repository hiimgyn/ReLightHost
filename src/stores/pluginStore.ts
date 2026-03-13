import { create } from 'zustand';
import type { PluginInfo, PluginInstanceInfo, PluginStatus } from '../lib/types';
import * as tauri from '../lib/tauri';

interface PluginStore {
  // State
  availablePlugins: PluginInfo[];
  pluginChain: PluginInstanceInfo[];
  crashStatusByInstanceId: Record<string, PluginStatus>;
  isScanning: boolean;
  isChainInitializing: boolean;
  hasFetchedChainOnce: boolean;
  mutationCount: number;
  isMutating: boolean;
  
  // Actions
  scanPlugins: () => Promise<void>;
  addToChain: (plugin: PluginInfo) => Promise<void>;
  removeFromChain: (instanceId: string) => Promise<void>;
  toggleBypass: (instanceId: string) => Promise<void>;
  reorderChain: (fromIndex: number, toIndex: number) => Promise<void>;
  fetchChain: () => Promise<void>;
  fetchCrashStatuses: () => Promise<void>;
}

export const usePluginStore = create<PluginStore>((set, get) => ({
  availablePlugins: [],
  pluginChain: [],
  crashStatusByInstanceId: {},
  isScanning: false,
  isChainInitializing: true,
  hasFetchedChainOnce: false,
  mutationCount: 0,
  isMutating: false,

  scanPlugins: async () => {
    set({ isScanning: true });
    try {
      const plugins = await tauri.scanPlugins();
      set({ availablePlugins: plugins });
    } catch (error) {
      console.error('Failed to scan plugins:', error);
    } finally {
      set({ isScanning: false });
    }
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

  toggleBypass: async (instanceId: string) => {
    const instance = get().pluginChain.find(p => p.instance_id === instanceId);
    if (!instance) return;

    set((state) => {
      const next = state.mutationCount + 1;
      return { mutationCount: next, isMutating: next > 0 };
    });
    try {
      await tauri.setPluginBypass(instanceId, !instance.bypassed);
      await get().fetchChain();
    } catch (error) {
      console.error('Failed to toggle bypass:', error);
      throw error;
    } finally {
      set((state) => {
        const next = Math.max(0, state.mutationCount - 1);
        return { mutationCount: next, isMutating: next > 0 };
      });
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
      set({ crashStatusByInstanceId: next });
    } catch (error) {
      console.error('Failed to fetch plugin crash statuses:', error);
    }
  },

}));
