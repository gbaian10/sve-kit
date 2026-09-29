import { useHref, useNavigate } from "react-router"

import { classStyle } from "../../domain/classes"
import { symbolIcon } from "../card/symbolIcons"
import { cn } from "../ui/cn"
import type { GridCell } from "./CardGrid"

export interface ListViewProps {
  readonly cells: readonly GridCell[]
  readonly onOpen: (cell: GridCell) => void
  readonly anchor?: string
}

function Row({
  cell,
  onOpen,
  highlighted,
}: {
  readonly cell: GridCell
  readonly onOpen: (cell: GridCell) => void
  readonly highlighted: boolean
}) {
  const navigate = useNavigate()
  const href = useHref(cell.to)
  const cost = cell.summary.cost
  const costIcon = cost === null ? undefined : symbolIcon("cost", String(cost))
  return (
    <li>
      <a
        href={href}
        data-result-key={cell.key}
        data-class={classStyle(cell.summary.classCode)}
        onClick={(event) => {
          if (
            event.defaultPrevented ||
            event.button !== 0 ||
            event.metaKey ||
            event.ctrlKey ||
            event.shiftKey ||
            event.altKey
          )
            return
          event.preventDefault()
          onOpen(cell)
          void navigate(cell.to, { state: cell.state })
        }}
        className={cn(
          "flex h-13 items-center gap-3 border-b border-l-[3px] border-border border-l-class-current pr-1 pl-3 text-text-1",
          highlighted && "rounded-card ring-2 ring-accent",
        )}
      >
        {cost === null ? (
          <span className="size-5.5 shrink-0" />
        ) : costIcon === undefined ? (
          <span className="flex size-5.5 shrink-0 items-center justify-center rounded-full bg-cost text-11 font-bold text-surface-1 tabular-nums">
            {cost}
          </span>
        ) : (
          <img src={costIcon} alt={String(cost)} className="size-5.5 shrink-0" />
        )}
        <span className="flex min-w-0 flex-1 flex-col leading-tight">
          <span lang={cell.name.primary.lang} className="truncate text-15 font-semibold">
            {cell.name.primary.text}
          </span>
          {cell.name.secondary && (
            <span lang={cell.name.secondary.lang} className="truncate text-12 text-text-2">
              {cell.name.secondary.text}
            </span>
          )}
        </span>
        {cell.summary.attack !== null && cell.summary.defense !== null && (
          <span className="shrink-0 text-14 font-semibold tabular-nums">
            {cell.summary.attack}/{cell.summary.defense}
          </span>
        )}
        <span aria-hidden="true" className="shrink-0 px-2 text-text-3">
          ›
        </span>
      </a>
    </li>
  )
}

// Design §6.2 list view: 52px rows, a 3px class-coloured left edge, cost icon, names, stats.
export function ListView({ cells, onOpen, anchor }: ListViewProps) {
  return (
    <ul>
      {cells.map((cell) => (
        <Row key={cell.key} cell={cell} onOpen={onOpen} highlighted={anchor === cell.key} />
      ))}
    </ul>
  )
}
