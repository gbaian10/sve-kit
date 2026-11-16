// The playmat: one picture under both players' zones, and the zones' printing on it. The picture
// is the player's own choice and is never shown to the other player.
//
// The printing follows the official playmat: zone names in English whatever the interface
// language, a base under every pile that shows while the pile is empty, and ornaments traced from
// the official mat (outlines only; the assets' README says where they come from). The pictures
// are drawn here, or are a card's art or a picture the player uploaded.
import { useId } from "react"

import ORNAMENTS from "../../../assets/card-labels/playmat/ornaments.json"
import type { CardRef } from "../../../battle/contract"
import { cn } from "../../../components/ui/cn"
import type { BattleMatLook } from "../../../settings"
import { type Box, CARD_ART, type Framing, placePicture, type Size } from "../hud/mat-crop"
import { ArtCrop } from "./Cards"
import { CARD_H, CARD_W, type MatGeometry, type PileName } from "./geometry"

const { shapes, words } = ORNAMENTS

interface Outline {
  readonly w: number
  readonly h: number
  readonly d: string
}

const num = (value: number): string => String(Math.round(value * 1e5) / 1e5)

/** An outline drawn `height` units tall around a point; a word's height is its cap height. */
function Outlined({
  outline,
  cx,
  cy,
  height,
  className,
  mirror = false,
  flip = false,
  traced = false,
}: {
  readonly outline: Outline
  readonly cx: number
  readonly cy: number
  readonly height: number
  readonly className: string
  readonly mirror?: boolean
  readonly flip?: boolean
  /** A traced shape fills by the even-odd rule; font outlines by the non-zero one. */
  readonly traced?: boolean
}) {
  const k = height / outline.h
  const sx = mirror ? -k : k
  const sy = flip ? -k : k
  return (
    <path
      d={outline.d}
      className={className}
      fillRule={traced ? "evenodd" : "nonzero"}
      transform={`translate(${num(cx - (sx * outline.w) / 2)} ${num(cy - (sy * outline.h) / 2)}) scale(${num(sx)} ${num(sy)})`}
    />
  )
}

interface Art {
  readonly id: string
  readonly w: number
  readonly h: number
  /** Where the picture's centre piece sits, as a share of the height. */
  readonly centre: number
  /** The largest radius the centre piece may take. */
  readonly ring: number
}

/** A night sky: a deep blue field, a faint honeycomb, a rune circle on the divider and stars. */
function Night({ id, w, h, centre, ring }: Art) {
  const cy = h * centre
  const r = Math.min(h * 0.33, ring)
  return (
    <>
      <defs>
        <radialGradient id={`${id}-g`} cx="50%" cy="40%" r="75%">
          <stop offset="0%" className="bp-night-stop-1" />
          <stop offset="60%" className="bp-night-stop-2" />
          <stop offset="100%" className="bp-night-stop-3" />
        </radialGradient>
        <pattern
          id={`${id}-hex`}
          width="34"
          height="58.9"
          patternUnits="userSpaceOnUse"
          patternTransform="scale(0.9)"
        >
          <path
            className="bp-night-hex"
            d="M17 0 L34 9.8 L34 29.4 L17 39.3 L0 29.4 L0 9.8 Z M17 39.3 L17 58.9"
          />
        </pattern>
      </defs>
      <rect width={w} height={h} fill={`url(#${id}-g)`} />
      <rect width={w} height={h} fill={`url(#${id}-hex)`} />
      <g className="bp-night-rune">
        <circle cx={w / 2} cy={cy} r={r} strokeWidth="1.5" />
        <circle cx={w / 2} cy={cy} r={r * 0.88} strokeWidth="0.8" strokeDasharray="3 7" />
        <path
          d={`M${num(w / 2)} ${num(cy - r)} L${num(w / 2 + r * 0.87)} ${num(cy + r / 2)} L${num(w / 2 - r * 0.87)} ${num(cy + r / 2)} Z`}
          strokeWidth="0.8"
        />
        <path
          d={`M${num(w / 2)} ${num(cy + r)} L${num(w / 2 + r * 0.87)} ${num(cy - r / 2)} L${num(w / 2 - r * 0.87)} ${num(cy - r / 2)} Z`}
          strokeWidth="0.8"
        />
      </g>
      {Array.from({ length: Math.round((w * h) / 12000) }, (_, i) => (
        <circle
          key={i}
          className="bp-night-star"
          cx={(i * 263) % w}
          cy={(i * 137) % h}
          r={i % 5 === 0 ? 1.6 : 0.9}
          opacity={0.25 + (i % 3) * 0.12}
        />
      ))}
    </>
  )
}

/** Plain deep blue, lit from the middle of the rows: the colours of the official playmat. */
function Blue({ id, w, h }: Art) {
  return (
    <>
      <defs>
        <radialGradient id={`${id}-g`} cx="42%" cy="46%" r="62%">
          <stop offset="0%" className="bp-blue-stop-1" />
          <stop offset="48%" className="bp-blue-stop-2" />
          <stop offset="100%" className="bp-blue-stop-3" />
        </radialGradient>
      </defs>
      <rect width={w} height={h} fill={`url(#${id}-g)`} />
    </>
  )
}

/** Green felt with a gold rim and a few vines around the centre. */
function Forest({ id, w, h, centre, ring }: Art) {
  const cy = h * centre
  return (
    <>
      <defs>
        <radialGradient id={`${id}-g`} cx="50%" cy="45%" r="80%">
          <stop offset="0%" className="bp-forest-stop-1" />
          <stop offset="70%" className="bp-forest-stop-2" />
          <stop offset="100%" className="bp-forest-stop-3" />
        </radialGradient>
        <filter id={`${id}-felt`}>
          <feTurbulence type="fractalNoise" baseFrequency="0.9" numOctaves="2" seed="3" />
          <feColorMatrix values="0 0 0 0 0  0 0 0 0 0  0 0 0 0 0  0 0 0 0.09 0" />
        </filter>
      </defs>
      <rect width={w} height={h} fill={`url(#${id}-g)`} />
      <rect width={w} height={h} filter={`url(#${id}-felt)`} />
      <g className="bp-forest-vine">
        {[0, 1, 2, 3, 4, 5].map((i) => (
          <path
            key={i}
            d={`M${num(w / 2 + Math.cos(i) * 40)} ${num(cy)} q ${num(60 + i * 20)} ${num(-40 - i * 8)} ${num(140 + i * 30)} ${num(-10 + i * 4)}`}
            transform={`rotate(${num(i * 60)} ${num(w / 2)} ${num(cy)})`}
          />
        ))}
        <ellipse
          cx={w / 2}
          cy={cy}
          rx={Math.min(w * 0.19, ring * 2.2)}
          ry={Math.min(h * 0.26, ring)}
        />
      </g>
      <rect x="6" y="6" width={w - 12} height={h - 12} rx="18" className="bp-forest-rim" />
    </>
  )
}

/** Pale parchment with a compass ring; the printing on it turns to brown ink. */
function Parchment({ id, w, h, centre, ring }: Art) {
  const cy = h * centre
  const r = Math.min(h * 0.26, ring)
  return (
    <>
      <defs>
        <radialGradient id={`${id}-g`} cx="50%" cy="50%" r="80%">
          <stop offset="0%" className="bp-parchment-stop-1" />
          <stop offset="75%" className="bp-parchment-stop-2" />
          <stop offset="100%" className="bp-parchment-stop-3" />
        </radialGradient>
        <filter id={`${id}-paper`}>
          <feTurbulence type="fractalNoise" baseFrequency="0.035" numOctaves="4" seed="7" />
          <feColorMatrix values="0 0 0 0 0.45  0 0 0 0 0.32  0 0 0 0 0.15  0 0 0 0.18 0" />
        </filter>
      </defs>
      <rect width={w} height={h} fill={`url(#${id}-g)`} />
      <rect width={w} height={h} filter={`url(#${id}-paper)`} />
      <g className="bp-parchment-ring">
        <circle cx={w / 2} cy={cy} r={r} strokeWidth="1.2" />
        <circle cx={w / 2} cy={cy} r={r * 0.93} strokeWidth="0.6" />
        {Array.from({ length: 12 }, (_, i) => (
          <line
            key={i}
            x1={w / 2}
            y1={cy - r * 0.93}
            x2={w / 2}
            y2={cy - r * 0.8}
            transform={`rotate(${num(i * 30)} ${num(w / 2)} ${num(cy)})`}
            strokeWidth="1"
          />
        ))}
      </g>
    </>
  )
}

function Picture({ look, ...art }: Art & { readonly look: BattleMatLook }) {
  if (look === "blue") return <Blue {...art} />
  if (look === "forest") return <Forest {...art} />
  if (look === "parchment") return <Parchment {...art} />
  if (look === "night") return <Night {...art} />
  // Also what lies under a card's art or an uploaded picture until it is in, or if it never is.
  return <rect width={art.w} height={art.h} className="bp-mat-plain" />
}

/**
 * A picture that is not drawn here: the art of a card from the snapshot, or the player's own.
 * Either is kept whole with the frame the player put on it; `anchor` is the box of the reach they
 * framed it for, in the coordinates of `box`, so that every reach looks at it in the same place.
 */
export type MatPhoto = (
  | { readonly kind: "card"; readonly card: CardRef }
  | { readonly kind: "upload"; readonly url: string; readonly size: Size }
) & { readonly framing: Framing; readonly anchor: Box }

/**
 * The mat's picture, in the box the screen gives it: the whole table between the side panels, or
 * the board only (`framed`: rounded and shadowed like a mat lying on the page). The two mats draw
 * their printing over it. Drawn pictures are set out in board units, so that their details are
 * the size they would be on a mat; a photo covers the box, blurred and darkened as the player set.
 */
export function MatArt({
  look,
  photo,
  blur,
  dim,
  box,
  u,
  centre,
  framed,
}: {
  readonly look: BattleMatLook
  /** Null when the look is a drawn one, and when the look's photo is not there. */
  readonly photo: MatPhoto | null
  readonly blur: number
  readonly dim: number
  readonly box: Box
  readonly u: number
  /** Where the line between the two mats runs, as a share of the box's height. */
  readonly centre: number
  readonly framed: boolean
}) {
  const id = useId()
  // A blurred picture overhangs its box, or its edges would fade to the mat under it.
  const over = 2 * blur
  const place = (size: Size, { framing, anchor }: MatPhoto) =>
    placePicture(size, framing, anchor, {
      left: box.left - over,
      top: box.top - over,
      width: box.width + 2 * over,
      height: box.height + 2 * over,
    })
  return (
    // The box cuts off whatever of a photo reaches past it: that is how a photo covers it.
    <div
      className={cn("bp-mat-art", framed && "is-framed")}
      style={{ ...box, borderRadius: framed ? 18 * u : undefined }}
      aria-hidden="true"
      data-hit-area=""
    >
      <svg
        className="bp-mat-svg"
        viewBox={`0 0 ${num(box.width / u)} ${num(box.height / u)}`}
        preserveAspectRatio="none"
      >
        <Picture
          look={look}
          id={id}
          w={box.width / u}
          h={box.height / u}
          centre={centre}
          ring={300}
        />
      </svg>
      {photo !== null ? (
        <>
          <div
            className="bp-mat-photo"
            data-hit-area=""
            style={{ inset: -over, filter: blur > 0 ? `blur(${num(blur)}px)` : undefined }}
          >
            {photo.kind === "card" ? (
              // The art fills a box of its own shape, which is placed like any other picture.
              <div className="bp-mat-img" style={place(CARD_ART, photo)}>
                <ArtCrop card={photo.card} />
              </div>
            ) : (
              <img
                className="bp-mat-img"
                src={photo.url}
                alt=""
                draggable={false}
                style={place(photo.size, photo)}
              />
            )}
          </div>
          <div className="bp-mat-veil" style={{ opacity: dim / 100 }} />
        </>
      ) : null}
    </div>
  )
}

const PILES: readonly PileName[] = ["leader", "deck", "cemetery", "evolve", "banish"]
const ROWS = ["field", "ex"] as const
/** The four corners of a pile's base: [right, bottom]. */
const CORNERS = [
  [false, false],
  [true, false],
  [false, true],
  [true, true],
] as const

// Sizes in mat units, measured on the official mat (a pile's base there is one card: 100 × 140).
// The row names and the divider are a fifth larger than there, to stay readable on a small screen.
const ROW_CAP = { field: 17, ex: 15.5 } as const
const ROW_ARROW_GAP = { field: 10, ex: 6 } as const
const ARROW_H = 6.6
const FLOURISH_H = 13
const ZONE_CAP = 7.7
const ZONE_BASELINE = 108.4
const CORNER = { inset: 3.8, size: 32 } as const

/** The mat's printing: zone names, the divider, slot marks and a base under every pile. */
export function ZoneFrames({ opp, mat }: { readonly opp: boolean; readonly mat: MatGeometry }) {
  const id = useId()
  const rows = mat.rowFrame("field", opp)
  const dividerY = opp ? mat.matH - mat.dividerY : mat.dividerY
  const mid = rows.x + rows.w / 2
  const flourishW = (shapes.flourish.w * FLOURISH_H) / shapes.flourish.h
  const arrowW = (shapes.arrow.w * ARROW_H) / shapes.arrow.h
  return (
    <svg
      className="bp-frames"
      viewBox={`0 0 ${num(mat.matW)} ${num(mat.matH)}`}
      preserveAspectRatio="none"
      aria-hidden="true"
    >
      <defs>
        <linearGradient id={`${id}-in`}>
          <stop offset="0%" className="bp-print-stop-faded" />
          <stop offset="55%" className="bp-print-stop" />
        </linearGradient>
        <linearGradient id={`${id}-out`}>
          <stop offset="45%" className="bp-print-stop" />
          <stop offset="100%" className="bp-print-stop-faded" />
        </linearGradient>
      </defs>
      {ROWS.map((row) => {
        const centre = mat.slotFrame(row, 2, opp)
        const cx = centre.x + centre.w / 2
        const cy = centre.y + CARD_H / 2
        const word = words[row]
        const reach = (word.w * ROW_CAP[row]) / word.h / 2 + ROW_ARROW_GAP[row] + arrowW / 2
        return (
          <g key={row}>
            {[0, 1, 2, 3, 4].map((slot) => {
              const s = mat.slotFrame(row, slot, opp)
              return (
                <rect
                  key={slot}
                  x={s.x + 4}
                  y={s.y + (opp ? -2 : 2)}
                  width={s.w - 8}
                  height={CARD_H}
                  rx="8"
                  className="bp-frame-slot"
                />
              )
            })}
            <g className="bp-print-row">
              <Outlined
                outline={word}
                cx={cx}
                cy={cy}
                height={ROW_CAP[row]}
                className="bp-print-label"
              />
              <Outlined
                outline={shapes.arrow}
                cx={cx - reach}
                cy={cy}
                height={ARROW_H}
                className="bp-print-accent"
                traced
              />
              <Outlined
                outline={shapes.arrow}
                cx={cx + reach}
                cy={cy}
                height={ARROW_H}
                className="bp-print-accent"
                mirror
                traced
              />
            </g>
          </g>
        )
      })}
      <rect
        x={rows.x + 24}
        y={dividerY - 0.5}
        width={Math.max(0, mid - flourishW / 2 + 3 - (rows.x + 24))}
        height="1"
        fill={`url(#${id}-in)`}
      />
      <rect
        x={mid + flourishW / 2 - 3}
        y={dividerY - 0.5}
        width={Math.max(0, rows.x + rows.w - 24 - (mid + flourishW / 2 - 3))}
        height="1"
        fill={`url(#${id}-out)`}
      />
      <Outlined
        outline={shapes.flourish}
        cx={mid}
        cy={dividerY}
        height={FLOURISH_H}
        className="bp-print-accent"
        traced
      />
      {PILES.map((pile) => {
        const r = mat.pile(pile, opp)
        // A small pile (a phone's strip) carries the same base, scaled.
        const f = r.w / CARD_W
        const cx = r.x + r.w / 2
        const corner = CORNER.size * f
        const reach = r.w / 2 - CORNER.inset * f - corner / 2
        const drop = r.h / 2 - CORNER.inset * f - corner / 2
        return (
          <g
            key={pile}
            className={cn({
              "bp-zone-leader": pile === "leader",
              "bp-zone-deck": pile === "deck",
              "bp-zone-cemetery": pile === "cemetery",
              "bp-zone-evolve": pile === "evolve",
              "bp-zone-banish": pile === "banish",
            })}
          >
            <rect
              x={r.x + 1.1}
              y={r.y + 1.1}
              width={r.w - 2.2}
              height={r.h - 2.2}
              rx={6 * f}
              className="bp-zone-base"
            />
            {pile === "cemetery" ? (
              <rect
                x={r.x + 5.8 * f}
                y={r.y + 5.8 * f}
                width={r.w - 11.6 * f}
                height={r.h - 11.6 * f}
                rx={3.5 * f}
                className="bp-zone-inner"
              />
            ) : null}
            {pile === "leader" || pile === "deck" || pile === "evolve" ? (
              <>
                {CORNERS.map(([right, bottom]) => (
                  <Outlined
                    key={`${String(right)}-${String(bottom)}`}
                    outline={shapes.corner}
                    cx={cx + (right ? reach : -reach)}
                    cy={r.y + r.h / 2 + (bottom ? drop : -drop)}
                    height={corner}
                    className="bp-zone-art"
                    mirror={right}
                    flip={bottom}
                    traced
                  />
                ))}
                <Outlined
                  outline={shapes.emblem}
                  cx={cx}
                  cy={r.y + 59.5 * f}
                  height={70 * f}
                  className="bp-zone-art"
                  traced
                />
              </>
            ) : null}
            {pile === "cemetery" ? (
              <Outlined
                outline={shapes.skull}
                cx={cx}
                cy={r.y + 60.7 * f}
                height={46 * f}
                className="bp-zone-art"
                traced
              />
            ) : null}
            <Outlined
              outline={words[pile]}
              cx={cx}
              cy={r.y + (ZONE_BASELINE - ZONE_CAP / 2) * f}
              height={ZONE_CAP * f}
              className="bp-zone-ink"
            />
          </g>
        )
      })}
      {(() => {
        const r = mat.pp(opp)
        return <rect x={r.x} y={r.y} width={r.w} height={r.h} rx="9" className="bp-frame-pp" />
      })()}
    </svg>
  )
}
