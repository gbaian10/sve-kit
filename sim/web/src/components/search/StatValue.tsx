import blankCost from "../../assets/card-labels/cost-blank.svg"
import atk0 from "../../assets/card-labels/counters/counter-atk-0-flat.svg"
import atk1 from "../../assets/card-labels/counters/counter-atk-1-flat.svg"
import atk2 from "../../assets/card-labels/counters/counter-atk-2-flat.svg"
import atk3 from "../../assets/card-labels/counters/counter-atk-3-flat.svg"
import atk4 from "../../assets/card-labels/counters/counter-atk-4-flat.svg"
import atk5 from "../../assets/card-labels/counters/counter-atk-5-flat.svg"
import atk6 from "../../assets/card-labels/counters/counter-atk-6-flat.svg"
import atk7 from "../../assets/card-labels/counters/counter-atk-7-flat.svg"
import atk8 from "../../assets/card-labels/counters/counter-atk-8-flat.svg"
import atk9 from "../../assets/card-labels/counters/counter-atk-9-flat.svg"
import atk10plus from "../../assets/card-labels/counters/counter-atk-10plus-flat.svg"
import hp0 from "../../assets/card-labels/counters/counter-hp-0-flat.svg"
import hp1 from "../../assets/card-labels/counters/counter-hp-1-flat.svg"
import hp2 from "../../assets/card-labels/counters/counter-hp-2-flat.svg"
import hp3 from "../../assets/card-labels/counters/counter-hp-3-flat.svg"
import hp4 from "../../assets/card-labels/counters/counter-hp-4-flat.svg"
import hp5 from "../../assets/card-labels/counters/counter-hp-5-flat.svg"
import hp6 from "../../assets/card-labels/counters/counter-hp-6-flat.svg"
import hp7 from "../../assets/card-labels/counters/counter-hp-7-flat.svg"
import hp8 from "../../assets/card-labels/counters/counter-hp-8-flat.svg"
import hp9 from "../../assets/card-labels/counters/counter-hp-9-flat.svg"
import hp10plus from "../../assets/card-labels/counters/counter-hp-10plus-flat.svg"
import { cn } from "../ui/cn"

const COUNTERS = {
  attack: [atk0, atk1, atk2, atk3, atk4, atk5, atk6, atk7, atk8, atk9, atk10plus],
  defense: [hp0, hp1, hp2, hp3, hp4, hp5, hp6, hp7, hp8, hp9, hp10plus],
} as const

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
  if (field === "cost")
    return (
      <span className="relative inline-flex h-7">
        <img src={blankCost} alt="" aria-hidden className="h-7 w-auto" />
        <b
          aria-hidden
          className={cn(
            INK,
            "absolute inset-0 flex items-center justify-center",
            orMore ? "text-11 tracking-tighter" : "text-15",
          )}
        >
          {orMore ? `${String(value)}+` : value}
        </b>
      </span>
    )
  const faces = COUNTERS[field]
  return (
    <img src={faces[Math.min(value, faces.length - 1)]} alt="" aria-hidden className="h-8 w-auto" />
  )
}
