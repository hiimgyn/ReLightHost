import { useMemo } from 'react';
import type { PluginInfo } from '../../lib/types';

type FormatFilter = 'all' | 'vst3' | 'vst' | 'clap' | 'builtin';

interface UsePluginLibraryFiltersOptions {
  availablePlugins: PluginInfo[];
  searchQuery: string;
  filterFormat: FormatFilter;
}

/** Search/format filtering, author grouping, and per-format counts for the plugin library. */
export function usePluginLibraryFilters({
  availablePlugins,
  searchQuery,
  filterFormat,
}: UsePluginLibraryFiltersOptions) {
  const filteredPlugins = useMemo(() => {
    const query = searchQuery.toLowerCase();
    return availablePlugins.filter((plugin) => {
      const matchesSearch =
        plugin.name.toLowerCase().includes(query) ||
        (plugin.manufacture?.toLowerCase().includes(query) ?? false) ||
        (plugin.category?.toLowerCase().includes(query) ?? false);
      const matchesFormat = filterFormat === 'all' || plugin.format === filterFormat;
      return matchesSearch && matchesFormat;
    });
  }, [availablePlugins, filterFormat, searchQuery]);

  // Group plugins by manufacture/author
  const groupedByAuthor = useMemo(() => filteredPlugins.reduce((acc: Record<string, PluginInfo[]>, plugin) => {
    const author = plugin.manufacture?.trim() || 'Unknown';
    if (!acc[author]) acc[author] = [];
    acc[author].push(plugin);
    return acc;
  }, {} as Record<string, PluginInfo[]>), [filteredPlugins]);
  const authorKeys = useMemo(() => Object.keys(groupedByAuthor).sort((a, b) => a.localeCompare(b)), [groupedByAuthor]);

  const builtinCount = useMemo(() => availablePlugins.filter(p => p.format === 'builtin').length, [availablePlugins]);
  const vst3Count = useMemo(() => availablePlugins.filter(p => p.format === 'vst3').length, [availablePlugins]);
  const vstCount = useMemo(() => availablePlugins.filter(p => p.format === 'vst').length, [availablePlugins]);
  const clapCount = useMemo(() => availablePlugins.filter(p => p.format === 'clap').length, [availablePlugins]);

  const tabItems = useMemo(() => [
    { key: 'all', label: `All (${availablePlugins.length})`, children: null },
    { key: 'builtin', label: `Built-in (${builtinCount})`, children: null },
    { key: 'vst3', label: `VST3 (${vst3Count})`, children: null },
    { key: 'vst', label: `VST2 (${vstCount})`, children: null },
    { key: 'clap', label: `CLAP (${clapCount})`, children: null },
  ], [availablePlugins.length, builtinCount, clapCount, vst3Count, vstCount]);

  return { filteredPlugins, groupedByAuthor, authorKeys, tabItems };
}
