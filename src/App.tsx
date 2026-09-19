import { lazy, Suspense, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { message, notification, Button } from 'antd';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { useAudioStore } from './stores/audioStore';
import { usePluginStore } from './stores/pluginStore';
import LoadingScreen from './components/layout/LoadingScreen';
import ErrorBoundary from './components/layout/ErrorBoundary';
import { getMinimizeToTray } from './lib/tauri';

const Layout = lazy(() => import('./components/layout'));
const PluginChain = lazy(() => import('./components/chain'));
const AudioSettings = lazy(() => import('./components/audio'));

function InstallUpdateButton() {
  const [installing, setInstalling] = useState(false);
  const handleInstall = async () => {
    setInstalling(true);
    try {
      await invoke('install_update');
    } catch (error) {
      console.error('Failed to install update:', error);
      setInstalling(false);
    }
  };
  return (
    <Button type="primary" size="small" loading={installing} onClick={handleInstall}>
      Install & Restart
    </Button>
  );
}

function App() {
  const [showFirstTimeAudio, setShowFirstTimeAudio] = useState(false);
  const [isBooting, setIsBooting] = useState(true);
  const bootStartRef = useRef(Date.now());
  const asioRetryRef = useRef(false);
  const updateCheckRef = useRef(false);
  const { syncFromBackend, fetchStatus, fetchDevices, toggleMonitoring } = useAudioStore();
  const [messageApi, contextHolder] = message.useMessage();
  const [notificationApi, notificationContextHolder] = notification.useNotification();

  // ── Session restore on mount ──────────────────────────────────────────────
  useEffect(() => {
    let asioRetryIds: ReturnType<typeof setTimeout>[] = [];
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

          // Truly first time — show the setup modal.
          setTimeout(() => setShowFirstTimeAudio(true), 600);
        }
      } catch (error) {
        console.error('Failed to restore session:', error);
        usePluginStore.getState().setRestoreTargetCount(null);
        setTimeout(() => setShowFirstTimeAudio(true), 600);
      } finally {
        if (!asioRetryRef.current) {
          const retryDelays = [1200, 3200, 5200];
          asioRetryRef.current = true;
          asioRetryIds = retryDelays.map((delay) =>
            setTimeout(async () => {
              const state = useAudioStore.getState();
              const isAsio = (state.selectedInputDevice ?? state.selectedDevice)?.startsWith('asio_');
              if (!isAsio) return;
              try {
                if (!state.status.is_monitoring) {
                  await state.toggleMonitoring(true);
                  await state.fetchStatus();
                }
              } catch (e) {
                console.warn('ASIO auto-start retry failed:', e);
              }
            }, delay)
          );
        }
        const elapsed = Date.now() - bootStartRef.current;
        const minBootMs = 700;
        const delay = Math.max(0, minBootMs - elapsed);
        setTimeout(() => setIsBooting(false), delay);
      }
    };

    restoreSession();
    return () => { asioRetryIds.forEach(clearTimeout); };
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // ── Auto-check for updates on startup ─────────────────────────────────────
  useEffect(() => {
    if (updateCheckRef.current) return;
    updateCheckRef.current = true;

    const checkForUpdate = async () => {
      try {
        const info = await invoke<{ available: boolean; version?: string; notes?: string }>(
          'check_for_update'
        );
        if (!info.available) return;

        notificationApi.open({
          key: `update-${info.version}`,
          message: `Update available: v${info.version}`,
          description: info.notes || 'A new version of ReLightHost is ready to install.',
          duration: 0,
          btn: <InstallUpdateButton />,
        });
      } catch (error) {
        // Silent — this is a background check; the Settings page still
        // exposes a manual "Check for updates" button that surfaces errors.
        console.warn('Background update check failed:', error);
      }
    };

    checkForUpdate();
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // ── Window resize / close listeners ──────────────────────────────────────
  useEffect(() => {
    const appWindow = getCurrentWindow();

    const closePromise = appWindow.onCloseRequested(async (event) => {
      event.preventDefault();

      let minimizeToTray = false;
      try {
        minimizeToTray = await getMinimizeToTray();
        localStorage.setItem('minimizeToTray', String(minimizeToTray));
      } catch (error) {
        console.warn('Failed to read minimize_to_tray during close:', error);
        minimizeToTray = localStorage.getItem('minimizeToTray') === 'true';
      }

      try { console.log('onCloseRequested fired; minimizeToTray=', minimizeToTray); } catch {}

      // Only intercept the close to hide to tray when the option is enabled.
      if (minimizeToTray) {
        await appWindow.hide();
        return;
      }

      // Otherwise close plugin GUIs first, then exit the app explicitly.
      // This avoids shutdown getting stuck in plugin teardown during Drop.
      try {
        await invoke('close_plugins');
      } catch (error) {
        console.error('Failed to close plugins before exit:', error);
      }
      await invoke('quit_app');
    });

    return () => {
      closePromise.then(fn => fn());
    };
  }, []);

  return (
    <ErrorBoundary>
    <Suspense fallback={<LoadingScreen />}>
      {contextHolder}
      {notificationContextHolder}
      <Layout>
        <main className="h-full w-full p-2.5 md:p-3.5 flex flex-col min-h-0 overflow-hidden">
          <PluginChain />
        </main>
        {showFirstTimeAudio && (
          <Suspense fallback={null}>
            <AudioSettings
              isOpen
              onClose={() => setShowFirstTimeAudio(false)}
            />
          </Suspense>
        )}
      </Layout>
      {isBooting && <LoadingScreen />}
    </Suspense>
    </ErrorBoundary>
  )
}

export default App
