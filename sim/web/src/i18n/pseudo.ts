import type { PostProcessorModule } from "i18next"

// Accented look-alikes keep the text readable while making untranslated strings obvious.
const ACCENTED: Readonly<Record<string, string>> = {
  A: "Á",
  B: "Ɓ",
  C: "Ç",
  D: "Ð",
  E: "É",
  F: "Ƒ",
  G: "Ĝ",
  H: "Ĥ",
  I: "Í",
  J: "Ĵ",
  K: "Ķ",
  L: "Ļ",
  M: "Ḿ",
  N: "Ñ",
  O: "Ó",
  P: "Þ",
  Q: "Ǫ",
  R: "Ŕ",
  S: "Š",
  T: "Ţ",
  U: "Ú",
  V: "Ṽ",
  W: "Ŵ",
  X: "Ẋ",
  Y: "Ý",
  Z: "Ž",
  a: "á",
  b: "ƀ",
  c: "ç",
  d: "ð",
  e: "é",
  f: "ƒ",
  g: "ĝ",
  h: "ĥ",
  i: "í",
  j: "ĵ",
  k: "ķ",
  l: "ļ",
  m: "ḿ",
  n: "ñ",
  o: "ó",
  p: "þ",
  q: "ǫ",
  r: "ŕ",
  s: "š",
  t: "ţ",
  u: "ú",
  v: "ṽ",
  w: "ŵ",
  x: "ẋ",
  y: "ý",
  z: "ž",
}
// German and Finnish run about a third longer than English; CJK strings grow the same way.
const EXPANSION = 0.35
const WIDE = /[\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}]/u

/**
 * Stress form of a UI string: accented, 35% longer, bracketed so cut-off ends are visible. The
 * padding comes in short space-separated runs, so it wraps like extra words instead of forming
 * one unbreakable tail.
 */
export function pseudoLocalize(text: string): string {
  const chars = Array.from(text)
  const accented = chars.map((char) => ACCENTED[char] ?? char).join("")
  const filler = WIDE.test(text) ? "〜" : "~"
  let extra = Math.ceil(chars.length * EXPANSION)
  const runs: string[] = []
  while (extra > 0) {
    runs.push(filler.repeat(Math.min(extra, 3)))
    extra -= 3
  }
  return `[${accented}${runs.length > 0 ? ` ${runs.join(" ")}` : ""}]`
}

/** i18next post-processor; enabled in dev with `?pseudo` in the URL. */
export const pseudoPostProcessor: PostProcessorModule = {
  type: "postProcessor",
  name: "pseudo",
  process: (value: string) => pseudoLocalize(value),
}
