import { useState, useEffect } from 'react';
import { X, Plus, Trash2, FolderOpen } from 'lucide-react';
import { open } from '@tauri-apps/plugin-dialog';

interface PluginSettingsProps {
  isOpen: boolean;
  onClose: () => void;
}

export default function PluginSettings({ isOpen, onClose }: PluginSettingsProps) {
  const [customPaths, setCustomPaths] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    if (isOpen) {
      loadPaths();
    }
  }, [isOpen]);

  const loadPaths = async () => {
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      const paths = await invoke<string[]>('get_custom_scan_paths');
      setCustomPaths(paths);
    } catch (error) {
      console.error('Failed to load custom paths:', error);
    }
  };

  const addPath = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: 'Select Plugin Directory',
      });

      if (selected && typeof selected === 'string') {
        const { invoke } = await import('@tauri-apps/api/core');
        await invoke('add_custom_scan_path', { path: selected });
        await loadPaths();
      }
    } catch (error) {
      console.error('Failed to add path:', error);
    }
  };

  const removePath = async (path: string) => {
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('remove_custom_scan_path', { path });
      await loadPaths();
    } catch (error) {
      console.error('Failed to remove path:', error);
    }
  };

  const rescanPlugins = async () => {
    setLoading(true);
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('scan_plugins');
      alert('Plugin scan completed!');
    } catch (error) {
      console.error('Failed to scan plugins:', error);
      alert('Failed to scan plugins: ' + error);
    } finally {
      setLoading(false);
    }
  };

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 bg-black/50 dark:bg-black/60 flex items-center justify-center z-[1100]">
      <div className="bg-white dark:bg-gray-800 rounded-lg shadow-xl w-full max-w-2xl max-h-[80vh] flex flex-col">
        {/* Header */}
        <div className="flex items-center justify-between p-4 border-b border-gray-200 dark:border-gray-700">
          <h2 className="text-xl font-bold text-gray-900 dark:text-white">Plugin Scan Paths</h2>
          <button
            onClick={onClose}
            className="p-1 text-gray-500 dark:text-gray-400 hover:text-gray-900 dark:hover:text-white transition-colors"
          >
            <X size={20} />
          </button>
        </div>

        {/* Content */}
        <div className="flex-1 overflow-y-auto p-6">
          {/* Info */}
          <div className="bg-blue-50 dark:bg-blue-900/30 border border-blue-200 dark:border-blue-700 rounded-lg p-4 mb-6">
            <p className="text-sm text-blue-700 dark:text-blue-200">
              Add custom directories where your VST3 and CLAP plugins are located.
              The app will scan these paths in addition to the default system paths.
            </p>
          </div>

          {/* Default Paths Info */}
          <div className="mb-6">
            <h3 className="text-sm font-semibold text-gray-600 dark:text-gray-400 mb-2">Default System Paths:</h3>
            <div className="bg-gray-100 dark:bg-gray-900 rounded-lg p-3 space-y-1 text-xs text-gray-600 dark:text-gray-500 font-mono">
              <div>C:\Program Files\Common Files\VST3</div>
              <div>C:\Program Files\Common Files\CLAP</div>
              <div>%LOCALAPPDATA%\Programs\Common\VST3</div>
              <div>%LOCALAPPDATA%\Programs\Common\CLAP</div>
            </div>
          </div>

          {/* Custom Paths */}
          <div className="mb-6">
            <div className="flex items-center justify-between mb-3">
              <h3 className="text-sm font-semibold text-gray-900 dark:text-white">Custom Scan Paths:</h3>
              <button
                onClick={addPath}
                className="flex items-center gap-2 px-3 py-1 bg-blue-600 hover:bg-blue-700 rounded text-sm text-white transition-colors"
              >
                <Plus size={14} />
                Add Path
              </button>
            </div>

            {customPaths.length > 0 ? (
              <div className="space-y-2">
                {customPaths.map((path, index) => (
                  <div
                    key={index}
                    className="flex items-center justify-between bg-gray-100 dark:bg-gray-900 rounded-lg p-3"
                  >
                    <div className="flex items-center gap-3 flex-1 min-w-0">
                      <FolderOpen className="text-blue-400 flex-shrink-0" size={18} />
                      <span className="text-sm text-gray-700 dark:text-gray-300 truncate font-mono">
                        {path}
                      </span>
                    </div>
                    <button
                      onClick={() => removePath(path)}
                      className="p-1 text-red-500 dark:text-red-400 hover:text-red-600 dark:hover:text-red-300 transition-colors flex-shrink-0 ml-2"
                      title="Remove path"
                    >
                      <Trash2 size={16} />
                    </button>
                  </div>
                ))}
              </div>
            ) : (
              <div className="text-center py-8 text-gray-600 dark:text-gray-500 bg-gray-100 dark:bg-gray-900 rounded-lg">
                <p className="text-sm">No custom paths configured</p>
                <p className="text-xs mt-1">Click "Add Path" to add a custom scan directory</p>
              </div>
            )}
          </div>

          {/* Rescan Button */}
          <div className="bg-gray-100 dark:bg-gray-900 rounded-lg p-4">
            <p className="text-sm text-gray-600 dark:text-gray-400 mb-3">
              After adding or removing paths, rescan to update the plugin library.
            </p>
            <button
              onClick={rescanPlugins}
              disabled={loading}
              className="w-full px-4 py-2 bg-green-600 hover:bg-green-700 disabled:bg-gray-400 dark:disabled:bg-gray-700 disabled:cursor-not-allowed text-white rounded transition-colors"
            >
              {loading ? 'Scanning...' : 'Rescan All Plugins'}
            </button>
          </div>
        </div>

        {/* Footer */}
        <div className="flex items-center justify-end gap-3 p-4 border-t border-gray-200 dark:border-gray-700">
          <button
            onClick={onClose}
            className="px-4 py-2 bg-gray-200 dark:bg-gray-700 text-gray-900 dark:text-white rounded hover:bg-gray-300 dark:hover:bg-gray-600 transition-colors"
          >
            Close
          </button>
        </div>
      </div>
    </div>
  );
}
