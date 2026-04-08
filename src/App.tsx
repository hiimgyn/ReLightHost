import { lazy, Suspense, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { message } from 'antd';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { useAudioStore } from './stores/audioStore';
import { usePluginStore } from './stores/pluginStore';

const Layout = lazy(() => import('./components/layout'));
const PluginChain = lazy(() => import('./components/chain'));
const AudioSettings = lazy(() => import('./components/audio'));

function App() {
  const [showFirstTimeAudio, setShowFirstTimeAudio] = useState(false);
  const { syncFromBackend, fetchStatus, fetchDevices, toggleMonitoring } = useAudioStore();
  const [messageApi, contextHolder] = message.useMessage();

  // ── Session restore on mount ──────────────────────────────────────────────
  useEffect(() => {
    const restoreSession = async () => {
      try {
        usePluginStore.getState().setRestoreTargetCount(null);
        const result = await invoke<{
          audio_restored: boolean;
          plugins_restored: number;
          needs_deferred_start: boolean;
        }>('restore_session');

        usePluginStore
          .getState()
          .setRestoreTargetCount(result.plugins_restored > 0 ? result.plugins_restored : null);

        // Startup race guard: ensure chain store is refreshed even if
        // plugin-chain-changed event was emitted before listener attached.
        await usePluginStore.getState().fetchChain();
        await usePluginStore.getState().fetchCrashStatuses();

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
            // Backend orchestrates a safe delayed start window.
            // Call immediately; backend will wait for its anti-crash deadline.
            try {
              await toggleMonitoring(true);
              await fetchStatus();
            } catch (e) {
              console.error('Deferred backend start failed:', e);
            }
          } else if (result.audio_restored) {
            await fetchStatus();
          }

          if (result.plugins_restored > 0) {
              messageApi.success(
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
        usePluginStore.getState().setRestoreTargetCount(null);
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

    // Use a ref to hold the minimize-to-tray preference so the close handler
    // reads the most recent value without racing the async invoke call.
    const minimizeToTrayRef = { current: localStorage.getItem('minimizeToTray') === 'true' } as { current: boolean };

    // Sync minimizeToTray from persisted config into localStorage on startup
    // and update the ref when the backend responds.
    invoke<boolean>('get_minimize_to_tray')
      .then(val => {
        minimizeToTrayRef.current = val;
        localStorage.setItem('minimizeToTray', String(val));
      })
      .catch(() => {});

    const closePromise = appWindow.onCloseRequested(async (event) => {
      const minimizeToTray = minimizeToTrayRef.current;

      // Debug log to help trace behavior.
      try { console.log('onCloseRequested fired; minimizeToTray=', minimizeToTray); } catch {}

      // Only intercept the close to hide to tray when the option is enabled.
      if (minimizeToTray) {
        event.preventDefault();
        await appWindow.hide();
      }
      // Otherwise allow the default close behavior to proceed.
    });

    return () => {
      closePromise.then(fn => fn());
    };
  }, []);

  return (
    <Suspense fallback={<div className="h-screen" />}>
      {contextHolder}
      <Layout>
        <div className="glass-panel rh-main-inner h-full w-full max-w-[1820px] mx-auto px-4 py-4 md:px-7 md:py-5">
          <PluginChain />
        </div>
        {showFirstTimeAudio && (
          <Suspense fallback={null}>
            <AudioSettings
              isOpen
              onClose={() => setShowFirstTimeAudio(false)}
            />
          </Suspense>
        )}
      </Layout>
    </Suspense>
  )
}

export default App
