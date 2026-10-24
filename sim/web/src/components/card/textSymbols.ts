import iconAct from "../../assets/official/texticon/icon_act.png"
import iconAdvanceStand from "../../assets/official/texticon/icon_advancestand.png"
import iconBishop from "../../assets/official/texticon/icon_bishop.png"
import iconDragon from "../../assets/official/texticon/icon_dragon.png"
import iconElf from "../../assets/official/texticon/icon_elf.png"
import iconEvolve from "../../assets/official/texticon/icon_evolve.png"
import iconFanfare from "../../assets/official/texticon/icon_fanfare.png"
import iconLastword from "../../assets/official/texticon/icon_lastword.png"
import iconNeutral from "../../assets/official/texticon/icon_neutral.png"
import iconNightmare from "../../assets/official/texticon/icon_nightmare.png"
import iconQ from "../../assets/official/texticon/icon_q.png"
import iconQuick from "../../assets/official/texticon/icon_quick.png"
import iconRoyal from "../../assets/official/texticon/icon_royal.png"
import iconStand from "../../assets/official/texticon/icon_stand.png"
import iconSyokuzi from "../../assets/official/texticon/icon_syokuzi.png"
import iconUB from "../../assets/official/texticon/icon_UB.png"
import iconWitch from "../../assets/official/texticon/icon_witch.png"
import iconRide from "../../assets/official/texticon/ride.png"
import { ATTACK_ICON, costIcon, costIconNeedsNumber, DEFENSE_ICON } from "./symbolIcons"

// The snapshot has no text_symbol rows yet; official stems connect JP alts, EN tokens and
// translated glossary labels. Keep unrecognized markers intact.
type SymbolKind = "keyword" | "stat" | "class" | "cost"

interface TextSymbol {
  readonly icon: string
  readonly kind: SymbolKind
}
interface SymbolDefinition extends TextSymbol {
  /** JP alt text and zh-Hant label, both exactly as they appear inside the braces. */
  readonly spellings: readonly string[]
  /** EN file stems; the EN site names a few files differently from the JP site. */
  readonly stems: readonly string[]
}

const DEFINITIONS: readonly SymbolDefinition[] = [
  {
    icon: iconFanfare,
    kind: "keyword",
    spellings: ["ファンファーレ", "入場曲"],
    stems: ["icon_fanfare"],
  },
  {
    icon: iconLastword,
    kind: "keyword",
    spellings: ["ラストワード", "謝幕曲"],
    stems: ["icon_lastword"],
  },
  { icon: iconEvolve, kind: "keyword", spellings: ["進化"], stems: ["icon_evolve"] },
  { icon: iconStand, kind: "keyword", spellings: ["起動"], stems: ["icon_stand"] },
  { icon: iconAct, kind: "keyword", spellings: ["アクト", "橫置"], stems: ["icon_act"] },
  {
    icon: iconAdvanceStand,
    kind: "keyword",
    spellings: ["アドバンス起動", "進階起動"],
    stems: ["icon_advancestand", "icon_adv"],
  },
  {
    icon: iconSyokuzi,
    kind: "keyword",
    spellings: ["食事", "用餐"],
    stems: ["icon_syokuzi", "icon_carrot"],
  },
  { icon: iconRide, kind: "keyword", spellings: ["憑依"], stems: ["ride", "icon_ride"] },
  { icon: iconQuick, kind: "keyword", spellings: ["クイック", "Quick"], stems: ["icon_quick"] },
  { icon: iconUB, kind: "keyword", spellings: ["UB"], stems: ["icon_UB"] },
  { icon: iconQ, kind: "keyword", spellings: ["Q"], stems: ["icon_q"] },
  { icon: ATTACK_ICON, kind: "stat", spellings: ["攻撃力", "攻擊力"], stems: ["icon_power"] },
  { icon: DEFENSE_ICON, kind: "stat", spellings: ["体力", "生命值"], stems: ["icon_hp"] },
  { icon: iconRoyal, kind: "class", spellings: ["ロイヤル", "皇家護衛"], stems: ["icon_royal"] },
  { icon: iconDragon, kind: "class", spellings: ["ドラゴン", "龍族"], stems: ["icon_dragon"] },
  { icon: iconElf, kind: "class", spellings: ["エルフ", "精靈"], stems: ["icon_elf"] },
  { icon: iconWitch, kind: "class", spellings: ["ウィッチ", "巫師"], stems: ["icon_witch"] },
  {
    icon: iconNightmare,
    kind: "class",
    spellings: ["ナイトメア", "夜魔"],
    stems: ["icon_nightmare"],
  },
  {
    icon: iconNeutral,
    kind: "class",
    spellings: ["ニュートラル", "中立"],
    stems: ["icon_neutral"],
  },
  { icon: iconBishop, kind: "class", spellings: ["ビショップ", "主教"], stems: ["icon_bishop"] },
]

const BY_SPELLING = new Map(DEFINITIONS.flatMap((d) => d.spellings.map((s) => [s, d] as const)))
const BY_STEM = new Map(DEFINITIONS.flatMap((d) => d.stems.map((s) => [s, d] as const)))
const COST_SPELLING = /^(?:コスト|消費)(\d{1,2}|X)$/
const COST_STEM = /^(?:icon_cost(\d{2})|COST (X))$/
const MARKER = /\{([^{}]*)\}|《([^《》]+)》/g

interface Span {
  /** UTF-16 offsets of the piece in the source text, so annotation ranges can be laid over it. */
  readonly start: number
  readonly end: number
}

export interface SymbolPiece extends Span {
  readonly kind: "symbol"
  readonly symbol: TextSymbol
  /** The text the marker stands for: what a copy of the text should contain. */
  readonly alt: string
  /** The alt without the EN site's square brackets, for the optional label beside the icon. */
  readonly label: string
}

export type InlinePiece = (Span & { readonly kind: "text"; readonly text: string }) | SymbolPiece

function costSymbol(value: string): TextSymbol | undefined {
  if (value === "X") return { icon: costIcon(Number.NaN), kind: "cost" }
  const number = Number(value)
  // The official crystals stop at 10; a higher number stays as text rather than show the wrong crystal.
  return costIconNeedsNumber(number) ? undefined : { icon: costIcon(number), kind: "cost" }
}

function resolve(inner: string): { readonly symbol: TextSymbol; readonly alt: string } | undefined {
  const bar = inner.indexOf("|")
  if (bar >= 0) {
    const stem = inner.slice(0, bar)
    const alt = inner.slice(bar + 1)
    const cost = COST_STEM.exec(stem)
    const symbol = cost ? costSymbol(cost[2] ?? String(Number(cost[1]))) : BY_STEM.get(stem)
    return symbol && { symbol, alt }
  }
  const spelling = inner.replace(/^《(.*)》$/u, "$1")
  const cost = COST_SPELLING.exec(spelling)
  const symbol = cost ? costSymbol(cost[1] ?? "") : BY_SPELLING.get(spelling)
  return symbol && { symbol, alt: inner }
}

/** Splits card text into plain runs and the official icons its markers stand for. */
export function inlinePieces(text: string): readonly InlinePiece[] {
  const pieces: InlinePiece[] = []
  let cursor = 0
  for (const match of text.matchAll(MARKER)) {
    const resolved = resolve(match[1] ?? match[2] ?? "")
    if (!resolved) continue
    const end = match.index + match[0].length
    if (match.index > cursor)
      pieces.push({
        kind: "text",
        text: text.slice(cursor, match.index),
        start: cursor,
        end: match.index,
      })
    pieces.push({
      kind: "symbol",
      symbol: resolved.symbol,
      alt: match[2] === undefined ? resolved.alt : match[0],
      label: resolved.alt.replace(/^\[(.*)\]$/, "$1"),
      start: match.index,
      end,
    })
    cursor = end
  }
  if (cursor < text.length)
    pieces.push({ kind: "text", text: text.slice(cursor), start: cursor, end: text.length })
  return pieces
}
