import { defineConfig } from 'astro/config';
import react from '@astrojs/react';
import sitemap from '@astrojs/sitemap';
import tailwindcss from '@tailwindcss/vite';

export default defineConfig({
  site: 'https://relighthost.hiimgyn.cloud',
  trailingSlash: 'ignore',
  i18n: {
    locales: ['en', 'vi'],
    defaultLocale: 'en',
    routing: { prefixDefaultLocale: false },
  },
  integrations: [
    react(),
    sitemap({ i18n: { defaultLocale: 'en', locales: { en: 'en', vi: 'vi' } } }),
  ],
  vite: { plugins: [tailwindcss()] },
});
