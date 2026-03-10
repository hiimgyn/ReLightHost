import { create } from 'zustand';
import type { PluginInfo, PluginInstanceInfo } from '../lib/types';
import * as tauri from '../lib/tauri';

interface PluginStore {
  // State
  availablePlugins: PluginInfo[];
  pluginChain: PluginInstanceInfo[];
  isScanning: boolean;
  
  // Actions
  scanPlugins: () => Promise<void>;
  addToChain: (plugin: PluginInfo) => Promise<void>;
  removeFromChain: (instanceId: string) => Promise<void>;
  toggleBypass: (instanceId: string) => Promise<void>;
  fetchChain: () => Promise<void>;
  reorderChain: (fromIndex: number, toIndex: number) => Promise<void>;
  setParameter: (instanceId: string, paramId: number, value: number) => Promise<void>;
  applyPreset: (name: string) => Promise<void>;
}

export const usePluginStore = create<PluginStore>((set, get) => ({
  availablePlugins: [],
  pluginChain: [],
  isScanning: false,

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
    try {
      await tauri.loadPlugin(plugin);
      await get().fetchChain();
      // Auto-save after adding plugin
      await tauri.autoSavePreset();
    } catch (error) {
      console.error('Failed to add plugin to chain:', error);
      throw error;
    }
  },

  removeFromChain: async (instanceId: string) => {
    try {
      await tauri.removePlugin(instanceId);
      await get().fetchChain();
      // Auto-save after removing plugin
      await tauri.autoSavePreset();
    } catch (error) {
      console.error('Failed to remove plugin from chain:', error);
      throw error;
    }
  },

  toggleBypass: async (instanceId: string) => {
    const instance = get().pluginChain.find(p => p.instance_id === instanceId);
    if (!instance) return;

    try {
      // Auto-save after toggling bypass
      await tauri.autoSavePreset();
      await tauri.setPluginBypass(instanceId, !instance.bypassed);
      await get().fetchChain();
    } catch (error) {
      console.error('Failed to toggle bypass:', error);
      throw error;
    }
  },

  fetchChain: async () => {
    try {
      const chain = await tauri.getPluginChain();
      set({ pluginChain: chain });
    } catch (error) {
      console.error('Failed to fetch plugin chain:', error);
    }
  },

  reorderChain: async (fromIndex: number, toIndex: number) => {
    // Optimistic UI update
    const chain = [...get().pluginChain];
    const [removed] = chain.splice(fromIndex, 1);
    chain.splice(toIndex, 0, removed);
    set({ pluginChain: chain });
    try {
      await tauri.reorderPluginChain(fromIndex, toIndex);
    } catch (error) {
      console.error('Failed to reorder chain:', error);
      // Revert on failure
      await get().fetchChain();
      throw error;
    }
  },

  setParameter: async (instanceId: string, paramId: number, value: number) => {
    // Optimistic local update
    set(state => ({
      pluginChain: state.pluginChain.map(p =>
        p.instance_id === instanceId
          ? { ...p, parameters: p.parameters.map(param => param.id === paramId ? { ...param, value } : param) }
          : p
      )
    }));
    try {
      await tauri.setPluginParameter(instanceId, paramId, value);
    } catch (error) {
      console.error('Failed to set parameter:', error);
      throw error;
    }
  },

  applyPreset: async (name: string) => {
    try {
      await tauri.applyPreset(name);
      await get().fetchChain();
    } catch (error) {
      console.error('Failed to apply preset:', error);
      throw error;
    }
  },
}));
