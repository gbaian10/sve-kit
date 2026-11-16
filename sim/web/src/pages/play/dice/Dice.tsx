// Markers that sit on the cards like the dice of a physical table: attack (blue spear), defense
// (red shield), the keyword dice and the cost crystal. The art is in assets/card-labels/, whose
// README says where it comes from.
import { useTranslation } from "react-i18next"

import costBlank from "../../../assets/card-labels/cost-blank.svg"
import atkBlank from "../../../assets/card-labels/dice/counter-atk-blank.svg"
import hpBlank from "../../../assets/card-labels/dice/counter-hp-blank.svg"
import kwAssail from "../../../assets/card-labels/dice/kw-assail.svg"
import kwAura from "../../../assets/card-labels/dice/kw-aura.svg"
import kwBane from "../../../assets/card-labels/dice/kw-bane.svg"
import kwDrain from "../../../assets/card-labels/dice/kw-drain.svg"
import kwGuard from "../../../assets/card-labels/dice/kw-guard.svg"
import kwIntimidate from "../../../assets/card-labels/dice/kw-intimidate.svg"
import NUMERALS from "../../../assets/card-labels/dice/numerals.json"
import { cn } from "../../../components/ui/cn"
import type { DieKeyword } from "./keywords"

interface Glyph {
  readonly d: string
  readonly box: readonly number[]
  readonly top: number
}
const GLYPHS: Readonly<Record<string, Glyph | string>> = NUMERALS

// Shape boxes in the traced 600 px die face; the blank SVGs use the same coordinates.
const SHAPES = {
  atk: { src: atkBlank, box: [96, 61, 435, 478], dy: 0 },
  hp: { src: hpBlank, box: [134, 90, 472, 517], dy: -6 },
} as const
const SHRINK: Readonly<Record<number, number>> = { 1: 1, 2: 0.74, 3: 0.7 }

function Numeral({ kind, label }: { readonly kind: "atk" | "hp"; readonly label: string }) {
  const [x0, y0, x1, y1] = SHAPES[kind].box
  const glyphs = Array.from(label)
    .map((ch) => GLYPHS[ch])
    .filter((glyph): glyph is Glyph => typeof glyph === "object")
  if (glyphs.length === 0) return null
  const h = 0.69 * (y1 - y0) * (SHRINK[glyphs.length] ?? 0.5)
  const wmax = 0.7 * (x1 - x0) * (glyphs.length >= 3 ? 1.15 : 1)
  const widths = glyphs.map((glyph) => (glyph.box[2] ?? 0) - (glyph.box[0] ?? 0))
  // Cinzel 700 outlines on one baseline; 714 = the figures' cap height in font units.
  let k = h / 714
  let gap = 0.06 * h
  let total = widths.reduce((sum, w) => sum + w * k, 0) + gap * (glyphs.length - 1)
  if (total > wmax) {
    k *= wmax / total
    gap *= wmax / total
    total = wmax
  }
  // Numerals at 80% so they stay inside the spear / shield.
  k *= 0.8
  gap *= 0.8
  total *= 0.8
  const base = (y0 + y1) / 2 + SHAPES[kind].dy + (714 * k) / 2
  let x = (x0 + x1) / 2 - total / 2
  return (
    <>
      {glyphs.map((glyph, index) => {
        const at = x
        x += (widths[index] ?? 0) * k + gap
        return (
          <path
            key={index}
            className="bp-counter-numeral"
            d={glyph.d}
            transform={`translate(${String(at)} ${String(base - glyph.top * k)}) scale(${String(k)})`}
            strokeWidth={18 / k}
            paintOrder="stroke"
            strokeLinejoin="round"
          />
        )
      })}
    </>
  )
}

/**
 * Attack (blue spear) or defense (red shield) showing the current value, like the dice: the number
 * on the die is all there is to see. The printed value it moved away from is said on hover and
 * read out, and stands in the info column.
 */
export function CounterBadge({
  kind,
  value,
  className,
  label,
  printed,
}: {
  readonly kind: "atk" | "hp"
  readonly value: number
  readonly className: string
  /** What the number is, for a reader who cannot see the shape. */
  readonly label: string
  /** The printed value, when the card is on the table and its value may have moved away from it. */
  readonly printed?: number | null | undefined
}) {
  const { t } = useTranslation()
  const [x0, y0, x1, y1] = SHAPES[kind].box
  const viewBox = [x0 - 10, y0 - 10, x1 - x0 + 20, y1 - y0 + 20]
  const changed = printed != null && printed !== value
  const spoken = changed
    ? t("play.stat.changed", { label, now: value, printed })
    : t("play.stat.value", { label, now: value })
  return (
    <span className={cn("bp-counter", className)} title={spoken}>
      <svg viewBox={viewBox.join(" ")} role="img" aria-label={spoken}>
        <image
          href={SHAPES[kind].src}
          x={viewBox[0]}
          y={viewBox[1]}
          width={viewBox[2]}
          height={viewBox[3]}
        />
        <Numeral kind={kind} label={String(Math.max(0, value))} />
      </svg>
    </span>
  )
}

const KW: Record<DieKeyword, string> = {
  guard: kwGuard,
  intimidate: kwIntimidate,
  bane: kwBane,
  drain: kwDrain,
  aura: kwAura,
  assail: kwAssail,
}

/** A keyword die sitting on the card, as on the physical table. */
export function KeywordDie({
  keyword,
  on,
}: {
  readonly keyword: DieKeyword
  readonly on: boolean
}) {
  const { t } = useTranslation()
  return (
    <span
      className={on ? "bp-kwdie is-on" : "bp-kwdie"}
      data-kw={keyword}
      title={t(`play.keyword.${keyword}`)}
    >
      <img src={KW[keyword]} alt="" aria-hidden="true" />
      <b>{t(`play.keyword.${keyword}`)}</b>
    </span>
  )
}

/**
 * Cost: our own blank crystal with the number on top (no official icon). Without a quoted number
 * the crystal says so with a question mark.
 */
export function CostBadge({ value }: { readonly value: number | null }) {
  const { t } = useTranslation()
  if (value === null)
    return (
      <span className="bp-cost" title={t("play.stat.costUnknown")}>
        <img src={costBlank} alt="" aria-hidden="true" />
        <span className="sr-only">{t("play.stat.costUnknown")}</span>
        <b aria-hidden="true">?</b>
      </span>
    )
  return (
    <span className="bp-cost">
      <img src={costBlank} alt="" aria-hidden="true" />
      <span className="sr-only">{t("play.stat.cost")}</span>
      <b>{value}</b>
    </span>
  )
}
