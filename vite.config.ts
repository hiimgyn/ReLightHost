import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [react()],
  
  // Vite options tailored for Tauri development
  clearScreen: false,
  
  // Tauri expects a fixed port
  server: {
    port: 5173,
    strictPort: true,
    watch: {
      // Tell vite to ignore watching `src-tauri`
      ignored: ['**/src-tauri/**'],
    },
  },
  build: {
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (id.includes('node_modules/antd') || id.includes('node_modules/@ant-design')) {
            return 'antd';
          }
          if (id.includes('node_modules/react') || id.includes('node_modules/react-dom')) {
            return 'react-vendor';
          }
          if (id.includes('node_modules/@tauri-apps')) {
            return 'tauri';
          }
          if (
            id.includes('/src/components/PluginLibrary') ||
            id.includes('/src/components/PluginInfoModal') ||
            id.includes('/src/components/PluginSettings') ||
            id.includes('/src/components/PresetManager') ||
            id.includes('/src/components/NoiseSuppressorGui') ||
            id.includes('/src/components/CompressorGui') ||
            id.includes('/src/components/VoiceGui') ||
            id.includes('/src/components/AudioSettings')
          ) {
            return 'plugin-modals';
          }
        },
      },
    },
  },
})
