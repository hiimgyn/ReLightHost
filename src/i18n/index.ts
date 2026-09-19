import enUS from 'antd/locale/en_US';
import viVN from 'antd/locale/vi_VN';
import { useLocaleStore } from '../stores/localeStore';
import { en } from './en';
import { vi } from './vi';
import type { SupportedLocale, TranslationSchema } from './types';

export { en, vi };
export type { SupportedLocale, TranslationSchema };
export { enUS, viVN };

export const dictionaries: Record<SupportedLocale, TranslationSchema> = {
  en,
  vi,
};

export type TranslationKey = {
  [K in keyof TranslationSchema]: `${K}.${Extract<keyof TranslationSchema[K], string>}`
}[keyof TranslationSchema];

export function getTranslation(
  locale: SupportedLocale,
  key: TranslationKey | string,
  params?: Record<string, string | number>
): string {
  const parts = key.split('.');
  const section = parts[0] as keyof TranslationSchema;
  const field = parts[1];

  const dict = dictionaries[locale] || dictionaries.en;
  let text = (dict[section] as Record<string, string> | undefined)?.[field];

  if (!text) {
    // Fallback to English dictionary or the key string itself
    text = (dictionaries.en[section] as Record<string, string> | undefined)?.[field] || key;
  }

  if (params) {
    for (const [pKey, pVal] of Object.entries(params)) {
      text = text.replace(new RegExp(`\\{${pKey}\\}`, 'g'), String(pVal));
    }
  }

  return text;
}

export function useTranslation() {
  const { locale, setLocale, toggleLocale } = useLocaleStore();

  const t = (key: TranslationKey, params?: Record<string, string | number>): string => {
    return getTranslation(locale, key, params);
  };

  const antdLocale = locale === 'vi' ? viVN : enUS;

  return {
    t,
    locale,
    setLocale,
    toggleLocale,
    antdLocale,
    dict: dictionaries[locale] || dictionaries.en,
  };
}
