// UI language. Rust decides it (`config.language`, or the Windows display
// language) and sends a `language` event when it changes; `t` always reads
// the current dictionary, so markup and $derived values update on their own.

import en from "./locales/en";
import zh, { type Messages } from "./locales/zh";

export type Lang = "zh" | "en";

const DICTIONARIES: Record<Lang, Messages> = { zh, en };

let current = $state<Lang>("zh");

export function setLang(lang: Lang) {
  current = lang;
  document.documentElement.lang = lang === "zh" ? "zh-CN" : "en";
}

/** The current language's text: `t.settings.title`, `t.count.lines(3)`. */
export const t = new Proxy({} as Messages, {
  get: (_, key) => DICTIONARIES[current][key as keyof Messages],
});
