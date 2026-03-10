import { useEffect, useState } from 'react';
import { getCurrentWindow, LogicalSize } from '@tauri-apps/api/window';
import Layout from './components/Layout'
import PluginChain from './components/PluginChain'
import AudioSettings from './components/AudioSettings'

const ASPECT_RATIO = 900 / 600; // 1.5  –  16:10 aspect ratio (width / height)

function App() {
  const [showFirstTimeAudio, setShowFirstTimeAudio] = useState(false);

  useEffect(() => {
    const appWindow = getCurrentWindow();
    let pending = false;

    const resizePromise = appWindow.onResized(async ({ payload }) => {
      if (pending) return;
      pending = true;
      try {
        const { width, height } = payload;
        const expectedH = Math.round(width / ASPECT_RATIO);
        if (Math.abs(height - expectedH) > 4) {
          await appWindow.setSize(new LogicalSize(width, expectedH));
        }
      } finally {
        pending = false;
      }
    });

    const closePromise = appWindow.onCloseRequested(async (event) => {
      if (localStorage.getItem('minimizeToTray') === 'true') {
        event.preventDefault();
        await appWindow.hide();
      }
    });

    // Show audio settings on first launch
    if (!localStorage.getItem('audioConfigured')) {
      const t = setTimeout(() => setShowFirstTimeAudio(true), 600);
      return () => {
        clearTimeout(t);
        resizePromise.then(fn => fn());
        closePromise.then(fn => fn());
      };
    }

    return () => {
      resizePromise.then(fn => fn());
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
