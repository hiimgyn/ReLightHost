import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import type { SupportedLocale } from '../i18n/types';

interface LocaleStore {
  locale: SupportedLocale;
  setLocale: (locale: SupportedLocale) => void;
  toggleLocale: () => void;
}

export const useLocaleStore = create<LocaleStore>()(
  persist(
    (set, get) => ({
      locale: 'en',
      setLocale: (locale: SupportedLocale) => set({ locale }),
      toggleLocale: () => {
        const next: SupportedLocale = get().locale === 'en' ? 'vi' : 'en';
        set({ locale: next });
      },
    }),
    {
      name: 'relight-locale-storage',
    }
  )
);
