import { inject, provide, ref, watch, type InjectionKey, type Ref } from "vue";
import type { Locale } from "../types/locale";
import zhCN from "./messages/zh-CN";
import en from "./messages/en";

const STORAGE_KEY = "rkdevtool-locale";

const messages = {
  "zh-CN": zhCN,
  en,
} as const;

type MessageTree = Record<string, unknown>;

const I18N_KEY: InjectionKey<{
  locale: Ref<Locale>;
  setLocale: (locale: Locale) => void;
  t: (key: string, params?: Record<string, string>) => string;
}> = Symbol("i18n");

const SUPPORTED_LOCALES: Locale[] = ["zh-CN", "en"];

function matchLocale(tag: string): Locale | undefined {
  const normalized = tag.toLowerCase().replace(/_/g, "-");
  if (normalized.startsWith("zh")) return "zh-CN";
  if (normalized.startsWith("en")) return "en";
  return undefined;
}

function detectSystemLocale(): Locale {
  const candidates =
    navigator.languages?.length > 0 ? navigator.languages : [navigator.language];

  for (const tag of candidates) {
    const matched = matchLocale(tag);
    if (matched) return matched;
  }

  return "en";
}

function detectLocale(): Locale {
  const saved = localStorage.getItem(STORAGE_KEY);
  if (saved && SUPPORTED_LOCALES.includes(saved as Locale)) return saved as Locale;
  return detectSystemLocale();
}

function lookup(tree: MessageTree, key: string): string | undefined {
  const parts = key.split(".");
  let node: unknown = tree;
  for (const part of parts) {
    if (node == null || typeof node !== "object" || !(part in node)) return undefined;
    node = (node as Record<string, unknown>)[part];
  }
  return typeof node === "string" ? node : undefined;
}

function applyParams(text: string, params?: Record<string, string>) {
  if (!params) return text;
  return Object.entries(params).reduce(
    (result, [name, value]) => result.replace(new RegExp(`\\{${name}\\}`, "g"), value),
    text,
  );
}

export function provideI18n() {
  const locale = ref<Locale>(detectLocale());

  function t(key: string, params?: Record<string, string>) {
    const text = lookup(messages[locale.value], key) ?? lookup(messages["zh-CN"], key) ?? key;
    return applyParams(text, params);
  }

  function setLocale(next: Locale) {
    locale.value = next;
    localStorage.setItem(STORAGE_KEY, next);
  }

  watch(
    locale,
    (value) => {
      document.documentElement.lang = value;
    },
    { immediate: true },
  );

  const api = { locale, setLocale, t };
  provide(I18N_KEY, api);
  return api;
}

export function useI18n() {
  const i18n = inject(I18N_KEY);
  if (!i18n) throw new Error("useI18n must be used after provideI18n");
  return i18n;
}
