import type { TextLang } from "./search"

// Card text markup (design 04/06, prototype `segments()`): `{記號}` is an official text icon,
// `{コスト2}` an icon with a parameter, `【守護】` a keyword (optionally `【名前_2】` with a parameter),
// `\n` a paragraph break. What a symbol or keyword *is* comes from the snapshot (`text_symbol`
// spellings per language, `keyword` names per language), never from a table in this file.
export type Segment =
  | { readonly kind: "text"; readonly text: string }
  | {
      readonly kind: "symbol"
      readonly code: string
      readonly symbolId: string
      /** The parsed parameter (`"2"`, `"X"`), when the spelling takes one. */
      readonly parameter?: string
      /** The literal inside the braces, for copy text and fallbacks. */
      readonly raw: string
    }
  | {
      readonly kind: "keyword"
      readonly keywordId: string
      readonly name: string
      readonly parameter?: string
    }
  | { readonly kind: "unknown"; readonly raw: string }
  | { readonly kind: "break" }

export interface SymbolSpelling {
  readonly symbolId: string
  readonly code: string
  readonly lang: TextLang
  readonly prefix: string
  readonly suffix: string
  readonly parse: "literal" | "uint" | "variable"
  /** Allowed exact values for `variable`, e.g. ["X"]. */
  readonly variables: readonly string[]
  /** Inclusive bounds of a `uint` parameter from the snapshot's parameter schema. */
  readonly minimum?: number
  readonly maximum?: number
}

export interface KeywordName {
  readonly keywordId: string
  readonly lang: TextLang
  readonly name: string
}

export interface CardTextVocabulary {
  readonly spellings: readonly SymbolSpelling[]
  readonly keywords: readonly KeywordName[]
}

const TOKEN = /\{([^{}]+)\}|【([^】]+)】|\n/gu
const KEYWORD_PARAMETER = /^(.+?)_(\d+)$/u

function matchSymbol(
  raw: string,
  lang: TextLang,
  spellings: readonly SymbolSpelling[],
): Segment | null {
  for (const spelling of spellings) {
    if (spelling.lang !== lang) continue
    if (!raw.startsWith(spelling.prefix) || !raw.endsWith(spelling.suffix)) continue
    const middle = raw.slice(spelling.prefix.length, raw.length - spelling.suffix.length)
    if (middle.length < 0) continue
    const base = { kind: "symbol" as const, code: spelling.code, symbolId: spelling.symbolId, raw }
    if (spelling.parse === "literal") {
      if (middle === "") return base
      continue
    }
    if (spelling.parse === "uint") {
      // Only digits, within the declared bounds and safe to represent; anything else stays raw.
      if (!/^\d+$/u.test(middle) || middle.length > 15) continue
      const value = Number(middle)
      if (!Number.isSafeInteger(value)) continue
      if (spelling.minimum !== undefined && value < spelling.minimum) continue
      if (spelling.maximum !== undefined && value > spelling.maximum) continue
      return { ...base, parameter: String(value) }
    }
    if (spelling.variables.includes(middle)) return { ...base, parameter: middle }
  }
  return null
}

function matchKeyword(
  raw: string,
  lang: TextLang,
  keywords: readonly KeywordName[],
): Segment | null {
  const parts = KEYWORD_PARAMETER.exec(raw)
  const name = parts?.[1] ?? raw
  const keyword = keywords.find((item) => item.lang === lang && item.name === name)
  if (!keyword) return null
  const parameter = parts?.[2]
  return {
    kind: "keyword",
    keywordId: keyword.keywordId,
    name,
    ...(parameter === undefined ? {} : { parameter }),
  }
}

/** Splits one text unit into render segments; unknown tokens stay visible as text. */
export function parseCardText(
  text: string,
  lang: TextLang,
  vocabulary: CardTextVocabulary,
): Segment[] {
  const out: Segment[] = []
  let last = 0
  for (const match of text.matchAll(TOKEN)) {
    if (match.index > last) out.push({ kind: "text", text: text.slice(last, match.index) })
    const [token, symbol, keyword] = match
    if (token === "\n") out.push({ kind: "break" })
    else if (symbol !== undefined)
      out.push(matchSymbol(symbol, lang, vocabulary.spellings) ?? { kind: "unknown", raw: token })
    else if (keyword !== undefined)
      out.push(matchKeyword(keyword, lang, vocabulary.keywords) ?? { kind: "unknown", raw: token })
    last = match.index + token.length
  }
  if (last < text.length) out.push({ kind: "text", text: text.slice(last) })
  return out
}

/** Plain text for copying: symbols by their copy pattern, keywords in their brackets. */
export function cardTextToPlain(
  segments: readonly Segment[],
  copyPattern: (symbolId: string) => string | undefined,
): string {
  return segments
    .map((segment) => {
      switch (segment.kind) {
        case "text":
          return segment.text
        case "break":
          return "\n"
        case "symbol": {
          const pattern = copyPattern(segment.symbolId)
          return pattern === undefined
            ? segment.raw
            : pattern.replaceAll(/\{[a-z_]+\}/gu, segment.parameter ?? "")
        }
        case "keyword":
          return `【${segment.name}${segment.parameter === undefined ? "" : `_${segment.parameter}`}】`
        case "unknown":
          return segment.raw
      }
    })
    .join("")
}
