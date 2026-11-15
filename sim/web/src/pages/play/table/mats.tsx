// The built-in playmat, drawn here (no official artwork). With a mat per player each mat carries
// its own picture and the opponent's copy is turned 180°; with one shared mat a single picture
// runs across both halves of the board and each side only draws its zone frames on it. A real mat
// image replaces the picture; the frames stay as they are.
import { useTranslation } from "react-i18next"

import { CARD_H, type MatGeometry, type PileName } from "./geometry"

/** A night sky: a deep blue field, a faint honeycomb, a rune circle on the divider and stars. */
function Night({
  id,
  w,
  h,
  centre,
  ring,
}: {
  readonly id: string
  readonly w: number
  readonly h: number
  /** Where the rune circle sits, as a share of the height. */
  readonly centre: number
  /** The largest radius the rune circle may take. */
  readonly ring: number
}) {
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
          d={`M${String(w / 2)} ${String(cy - r)} L${String(w / 2 + r * 0.87)} ${String(cy + r / 2)} L${String(w / 2 - r * 0.87)} ${String(cy + r / 2)} Z`}
          strokeWidth="0.8"
        />
        <path
          d={`M${String(w / 2)} ${String(cy + r)} L${String(w / 2 + r * 0.87)} ${String(cy - r / 2)} L${String(w / 2 - r * 0.87)} ${String(cy - r / 2)} Z`}
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

/** One player's mat picture. */
export function MatPicture({ opp, mat }: { readonly opp: boolean; readonly mat: MatGeometry }) {
  return (
    <div className="bp-mat-art" style={opp ? { transform: "rotate(180deg)" } : undefined}>
      <svg
        className="bp-mat-svg"
        viewBox={`0 0 ${String(mat.matW)} ${String(mat.matH)}`}
        preserveAspectRatio="none"
        aria-hidden="true"
      >
        <Night
          id={`mat-night-${opp ? "o" : "s"}`}
          w={mat.matW}
          h={mat.matH}
          centre={mat.dividerY / mat.matH}
          ring={150}
        />
      </svg>
    </div>
  )
}

/** One mat for both players: a single picture across the whole board, its emblem in the middle. */
export function SharedMatPicture({
  mat,
  boardH,
}: {
  readonly mat: MatGeometry
  readonly boardH: number
}) {
  return (
    <div className="bp-mat-art">
      <svg
        className="bp-mat-svg"
        viewBox={`0 0 ${String(mat.matW)} ${String(boardH)}`}
        preserveAspectRatio="none"
        aria-hidden="true"
      >
        <Night id="mat-night-shared" w={mat.matW} h={boardH} centre={0.5} ring={300} />
      </svg>
    </div>
  )
}

const PILES: readonly PileName[] = ["leader", "deck", "cemetery", "evolve", "banish"]
const ROWS = ["field", "ex"] as const

/** Zone outlines and labels; labels stay upright on the opponent's mat. */
export function ZoneFrames({ opp, mat }: { readonly opp: boolean; readonly mat: MatGeometry }) {
  const { t } = useTranslation()
  const label = (
    x: number,
    y: number,
    text: string,
    anchor: "start" | "middle" | "end" = "middle",
  ) => (
    <text x={x} y={y} textAnchor={anchor} className="bp-frame-label">
      {text}
    </text>
  )
  const field = mat.rowFrame("field", opp)
  const ex = mat.rowFrame("ex", opp)
  return (
    <svg
      className="bp-frames"
      viewBox={`0 0 ${String(mat.matW)} ${String(mat.matH)}`}
      preserveAspectRatio="none"
      aria-hidden="true"
    >
      {ROWS.map((row) => {
        const r = row === "field" ? field : ex
        return (
          <g key={row}>
            <rect x={r.x} y={r.y} width={r.w} height={r.h} rx="12" className="bp-frame-row" />
            {label(
              opp ? r.x + r.w - 10 : r.x + 10,
              opp ? r.y + r.h - 6 : r.y + 16,
              row === "field" ? t("play.zone.field") : t("play.zone.ex"),
              opp ? "end" : "start",
            )}
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
          </g>
        )
      })}
      <line
        x1={field.x + 40}
        x2={field.x + field.w - 40}
        y1={opp ? mat.matH - mat.dividerY : mat.dividerY}
        y2={opp ? mat.matH - mat.dividerY : mat.dividerY}
        className="bp-frame-divider"
      />
      {PILES.map((pile) => {
        const r = mat.pile(pile, opp)
        const cx = r.x + r.w / 2
        const cy = r.y + r.h / 2 - 8
        return (
          <g key={pile} className={`bp-frame-pile-${pile}`}>
            <rect x={r.x} y={r.y} width={r.w} height={r.h} rx="9" className="bp-frame-pile" />
            <rect
              x={r.x + 5}
              y={r.y + 5}
              width={r.w - 10}
              height={r.h - 10}
              rx="6"
              className="bp-frame-pile-inner"
            />
            {pile === "cemetery" ? (
              // Our own skull glyph.
              <g className="bp-frame-icon" transform={`translate(${String(cx)} ${String(cy)})`}>
                <path d="M-15 -2 a15 15 0 1 1 30 0 v8 a4 4 0 0 1 -4 4 h-2 v6 h-18 v-6 h-2 a4 4 0 0 1 -4 -4 z" />
                <circle cx="-6" cy="0" r="4.2" className="bp-frame-icon-hole" />
                <circle cx="6" cy="0" r="4.2" className="bp-frame-icon-hole" />
                <path d="M-2 8 l2 -4 l2 4 z" className="bp-frame-icon-hole" />
              </g>
            ) : pile === "banish" ? null : (
              <g
                className="bp-frame-icon is-line"
                transform={`translate(${String(cx)} ${String(cy)})`}
              >
                <path d="M0 -18 L5 -5 L18 0 L5 5 L0 18 L-5 5 L-18 0 L-5 -5 Z" />
                <circle r="7" />
              </g>
            )}
            {pile === "leader" ? label(cx, r.y + r.h - 14, t("play.zone.leader")) : null}
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
