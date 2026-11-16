import { useTranslation } from "react-i18next"

import type { TableCard } from "../../../battle/view/table"
import { cn } from "../../../components/ui/cn"
import { counterChipUrl, useSnapshotRoot } from "../../../data"
import { useCounterText } from "./counters"
import { markArtFailed, useArtFailed } from "./published-art"

/** The drawn chip, shown where the published one cannot be loaded. */
function DrawnChip() {
  return (
    <svg className="bp-token-art" viewBox="0 0 64 64" aria-hidden="true">
      <circle className="bp-token-rim" cx="32" cy="32" r="31" />
      <g className="bp-token-notch">
        <path d="M32 1v8M32 55v8M1 32h8M55 32h8" />
        <path d="M10 10l6 6M48 48l6 6M54 10l-6 6M16 48l-6 6" />
      </g>
      <circle className="bp-token-face" cx="32" cy="32" r="21" />
      <circle className="bp-token-ring" cx="32" cy="32" r="17" />
    </svg>
  )
}

function Token({
  counter,
  side,
}: {
  readonly counter: TableCard["counters"][number]
  readonly side: "a" | "b"
}) {
  const { t } = useTranslation()
  const text = useCounterText()(counter)
  const url = counterChipUrl(side, useSnapshotRoot())
  const failed = useArtFailed(url)
  return (
    <span
      className="bp-token"
      role="img"
      aria-label={t("play.counter.spoken", { text })}
      title={text}
      // The number hangs over the chip's edge, outside its box; the overflow audit skips boxes
      // marked `data-hit-area`.
      data-hit-area=""
    >
      {failed ? (
        <DrawnChip />
      ) : (
        <img
          className="bp-token-art"
          src={url}
          alt=""
          draggable={false}
          onError={() => {
            markArtFailed(url)
          }}
        />
      )}
      <b className="bp-token-count">{counter.amount}</b>
    </span>
  )
}

/**
 * The counters on a card, as chips: one chip per kind with how many on it. A card holds one kind
 * at most today; were there more, the chips would lie side by side, alternating the chip's back
 * and front. The kind's name is said on hover and in the info column, not printed on the chip.
 */
export function CounterTokens({ card }: { readonly card: TableCard }) {
  if (card.counters.length === 0) return null
  return (
    // The numbers hang over the chips' edges, outside this row's box as well.
    <span
      className={cn(
        "bp-tokens",
        card.counters.length === 2 && "is-several",
        card.counters.length > 2 && "is-many",
      )}
      data-hit-area=""
    >
      {card.counters.map((counter, index) => (
        <Token key={counter.name} counter={counter} side={index % 2 === 0 ? "b" : "a"} />
      ))}
    </span>
  )
}
