import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { message } from 'antd';
import { getCurrentWindow } from '@tauri-apps/api/window';
import Layout from './components/Layout'
import PluginChain from './components/PluginChain'
import AudioSettings from './components/AudioSettings'
import { useAudioStore } from './stores/audioStore';

function App() {
  const [showFirstTimeAudio, setShowFirstTimeAudio] = useState(false);
  const { syncFromBackend, fetchStatus, fetchDevices, toggleMonitoring } = useAudioStore();

  // ── Session restore on mount ──────────────────────────────────────────────
  useEffect(() => {
    const restoreSession = async () => {
      try {
        const result = await invoke<{
          audio_restored: boolean;
          plugins_restored: number;
          needs_deferred_start: boolean;
        }>('restore_session');

        // Sync the frontend store so AudioSettings shows the restored values.
        await syncFromBackend();
        await fetchStatus();
        // Pre-fetch device list so AudioSettings host-type detection works
        // immediately when the user opens the modal.
        fetchDevices();

        if (result.audio_restored || result.plugins_restored > 0) {
          // Session found — suppress the first-time setup modal.
          localStorage.setItem('audioConfigured', 'true');

          if (result.needs_deferred_start) {
            // Voicemeeter ASIO Insert: wait 2 s for Voicemeeter to finish its
            // own startup, then connect on the Tauri command thread (which has
            // COM initialized — raw OS threads crash ASIO with AV).
            setTimeout(async () => {
              try {
                await toggleMonitoring(true);
                await fetchStatus();
              } catch (e) {
                console.error('Deferred ASIO start failed:', e);
              }
            }, 2000);
          } else if (result.audio_restored) {
            await fetchStatus();
          }

          if (result.plugins_restored > 0) {
            message.success(
              `Session restored — ${result.plugins_restored} plugin${result.plugins_restored > 1 ? 's' : ''} loaded`,
              4
            );
          }
        } else {
          // No session.json — try to start the stream with whatever device is
          // currently configured (covers the case where the user configured audio
          // previously but the session file was deleted).
          // This runs AFTER restore_session has fully completed, so there is no
          // race with the session restore path.
          try { await toggleMonitoring(true); } catch { /* no device configured */ }

          if (!localStorage.getItem('audioConfigured')) {
            // Truly first time — show the setup modal.
            setTimeout(() => setShowFirstTimeAudio(true), 600);
          }
        }
      } catch (error) {
        console.error('Failed to restore session:', error);
        if (!localStorage.getItem('audioConfigured')) {
          setTimeout(() => setShowFirstTimeAudio(true), 600);
        }
      }
    };

    restoreSession();
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // ── Window resize / close listeners ──────────────────────────────────────
  useEffect(() => {
    const appWindow = getCurrentWindow();
    // Sync minimizeToTray from persisted config into localStorage on startup.
    invoke<boolean>('get_minimize_to_tray').then(val => {
      localStorage.setItem('minimizeToTray', String(val));
    }).catch(() => {});

    const closePromise = appWindow.onCloseRequested(async (event) => {
      // Always prevent default in async handlers — then decide explicitly.
      event.preventDefault();
      if (localStorage.getItem('minimizeToTray') === 'true') {
        await appWindow.hide();
      } else {
        await appWindow.destroy();
      }
    });

    return () => {
      closePromise.then(fn => fn());
    };
  }, []);

  return (
    <Layout>
      <div className="h-full p-6">
        <PluginChain />
      </div>
      {showFirstTimeAudio && (
        <AudioSettings
          isOpen
          onClose={() => setShowFirstTimeAudio(false)}
        />
      )}
    </Layout>
  )
}

export default App
