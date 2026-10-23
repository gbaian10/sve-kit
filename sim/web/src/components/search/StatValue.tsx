import attackFace from "../../assets/card-labels/attack.svg"
import blankCost from "../../assets/card-labels/cost-blank.svg"
import defenseFace from "../../assets/card-labels/defense.svg"
import { cn } from "../ui/cn"

// The filter's stat buttons share one look: a blank cost crystal (matching the official text
// icons), the card-face attack gem and the defense shield, each with the number written on top.
// `nudge` centres the number on the shape (the gem sits above its cross-shaped base).
const FACES = {
  cost: { src: blankCost, size: "h-7", nudge: "" },
  attack: { src: attackFace, size: "h-8", nudge: "pb-[9%]" },
  defense: { src: defenseFace, size: "h-8", nudge: "pt-[3%]" },
} as const

// Lining figures keep every digit on one baseline (serif faces default to old-style figures).
const INK =
  "font-serif leading-none font-bold lining-nums tabular-nums text-stat-ink text-shadow-sm text-shadow-stat-ink-shadow"

export function StatValue({
  field,
  value,
  orMore,
}: {
  readonly field: "cost" | "attack" | "defense"
  readonly value: number
  /** The last button also matches every higher value, shown as "10+" inside the icon. */
  readonly orMore: boolean
}) {
  const face = FACES[field]
  return (
    <span className={cn("relative inline-flex", face.size)}>
      <img src={face.src} alt="" aria-hidden className={cn("w-auto", face.size)} />
      <b
        aria-hidden
        className={cn(
          INK,
          "absolute inset-0 flex items-center justify-center",
          orMore ? "text-11 tracking-tighter" : "text-15",
          face.nudge,
        )}
      >
        {orMore ? `${String(value)}+` : value}
      </b>
    </span>
  )
}
