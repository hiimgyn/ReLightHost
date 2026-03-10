import { useEffect } from 'react';
import { getCurrentWindow, LogicalSize } from '@tauri-apps/api/window';
import Layout from './components/Layout'
import PluginChain from './components/PluginChain'

const ASPECT_RATIO = 900 / 600; // 1.5  –  16:10 aspect ratio (width / height)

function App() {
  useEffect(() => {
    const appWindow = getCurrentWindow();
    let pending = false;

    const unlistenPromise = appWindow.onResized(async ({ payload }) => {
      if (pending) return;
      pending = true;
      try {
        const { width, height } = payload;
        const expectedH = Math.round(width / ASPECT_RATIO);
        // Only correct when the deviation is non-trivial (> 4 logical px)
        if (Math.abs(height - expectedH) > 4) {
          await appWindow.setSize(new LogicalSize(width, expectedH));
        }
      } finally {
        pending = false;
      }
    });

    return () => {
      unlistenPromise.then(fn => fn());
    };
  }, []);

  return (
    <Layout>
      <div className="h-full p-6">
        <PluginChain />
      </div>
    </Layout>
  )
}

export default App
