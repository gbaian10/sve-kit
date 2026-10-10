import { type MouseEvent, useState } from "react"
import { useTranslation } from "react-i18next"
import { useHref, useNavigate } from "react-router"

import type { CardSummary, ImageIndex } from "../../data"
import type { DisplayedName } from "../../domain/nameDisplay"
import { usePrefs } from "../../settings"
import { CardImage } from "../card/CardImage"
import { imageHeldBack } from "../card/image-state"
import { cn } from "../ui/cn"

export interface GridCell {
  readonly key: string
  readonly summary: CardSummary
  readonly name: DisplayedName
  readonly to: string
  readonly state: unknown
}

export interface CardGridProps {
  readonly cells: readonly GridCell[]
  readonly images: ImageIndex | undefined
  /** Called before the router navigates, so the list can record its anchor. */
  readonly onOpen: (cell: GridCell, event: MouseEvent<HTMLAnchorElement>) => void
  /** Cell to highlight briefly after returning from a card. */
  readonly anchor?: string
}

// Design: 2 columns under 600 (gap 14×12), 4 up to 999, 5–6 on desktop; each cell is the image
// plus one truncated 13px name line ("both" adds the translation in small type).
const GRID_SIZES =
  "(min-width: 1440px) 200px, (min-width: 1000px) 20vw, (min-width: 600px) 25vw, 50vw"

function CardCell({
  cell,
  images,
  onOpen,
  highlighted,
}: {
  readonly cell: GridCell
  readonly images: ImageIndex | undefined
  readonly onOpen: CardGridProps["onOpen"]
  readonly highlighted: boolean
}) {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const href = useHref(cell.to)
  const { dataSaver } = usePrefs()
  const [imageWanted, setImageWanted] = useState(false)
  const offerImage = dataSaver && !imageWanted && imageHeldBack(images, cell.summary)
  return (
    <span className="relative block">
      <a
        href={href}
        data-result-key={cell.key}
        onClick={(event) => {
          // Modified clicks (new tab, middle button) keep the browser's default.
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
          onOpen(cell, event)
          void navigate(cell.to, { state: cell.state })
        }}
        className={cn(
          "flex flex-col gap-1.5 rounded-card text-text-1 hover:opacity-90",
          highlighted && "ring-2 ring-accent ring-offset-2 ring-offset-bg",
        )}
      >
        <CardImage
          summary={cell.summary}
          name={cell.name.primary}
          images={images}
          alt="redundant"
          sizes={GRID_SIZES}
          className="w-full"
          imageWanted={imageWanted}
        />
        <span className="flex min-w-0 flex-col leading-tight">
          <span className="flex items-baseline gap-2">
            <span
              lang={cell.name.primary.lang}
              className="min-w-0 flex-1 truncate text-13 font-medium"
            >
              {cell.name.primary.text}
            </span>
            <span className="shrink-0 text-11 whitespace-nowrap text-text-3 tabular-nums">
              {cell.summary.cardNo}
            </span>
          </span>
          {cell.name.secondary && (
            <span lang={cell.name.secondary.lang} className="truncate text-11 text-text-3">
              {cell.name.secondary.text}
            </span>
          )}
          {cell.name.missingTranslation && (
            <span className="truncate text-11 text-text-3">{t("card.noTranslation")}</span>
          )}
          {cell.name.lowConfidence && (
            <span className="text-11 text-text-3">{t("card.translationProofreading")}</span>
          )}
          {cell.name.jpSource && <span className="text-11 text-text-3">{t("card.jpSource")}</span>}
          {cell.summary.wordingPending && (
            <span className="mt-1 self-start rounded-control border border-border-strong px-1.5 py-0.5 text-11 text-text-2">
              {t("card.wordingPending")}
            </span>
          )}
        </span>
      </a>
      {offerImage && (
        <button
          type="button"
          onClick={() => {
            setImageWanted(true)
          }}
          className="absolute inset-x-[12%] top-[62%] rounded-control border border-border-strong bg-surface-1 px-2 py-1.5 text-12 font-semibold text-text-1"
        >
          {t("card.loadImage")}
        </button>
      )}
    </span>
  )
}

export function CardGrid({ cells, images, onOpen, anchor }: CardGridProps) {
  return (
    <ul className="grid grid-cols-2 gap-x-3 gap-y-3.5 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6">
      {cells.map((cell) => (
        <li key={cell.key} className="min-w-0">
          <CardCell cell={cell} images={images} onOpen={onOpen} highlighted={anchor === cell.key} />
        </li>
      ))}
    </ul>
  )
}
