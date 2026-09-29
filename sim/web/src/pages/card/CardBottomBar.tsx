import { ChevronLeft, ChevronRight, Plus } from "lucide-react"
import { useTranslation } from "react-i18next"

import { BottomBar } from "../../components/nav/BottomBar"
import { cn } from "../../components/ui/cn"

export interface CardBottomBarProps {
  readonly onPrev: (() => void) | undefined
  readonly onNext: (() => void) | undefined
}

// Overlay bottom bar (design 03a): previous / add to deck (disabled until M4) / next.
export function CardBottomBar({ onPrev, onNext }: CardBottomBarProps) {
  const { t } = useTranslation()
  const arrow = (
    label: string,
    onClick: (() => void) | undefined,
    Icon: typeof ChevronLeft,
    iconAfter: boolean,
  ) => (
    <button
      type="button"
      onClick={onClick}
      disabled={onClick === undefined}
      className={cn(
        "flex h-14 min-w-11 flex-1 items-center justify-center gap-1 text-14 font-semibold text-text-1 disabled:text-text-3",
        iconAfter && "flex-row-reverse",
      )}
    >
      <Icon className="size-5 shrink-0" aria-hidden="true" />
      <span className="truncate">{label}</span>
    </button>
  )
  return (
    <BottomBar className="z-50 lg:pl-18">
      <div className="mx-auto flex h-15.5 max-w-320 items-stretch px-2 lg:px-6">
        {arrow(t("cardPage.prev"), onPrev, ChevronLeft, false)}
        <button
          type="button"
          disabled
          title={t("cardPage.addToDeckSoon")}
          className="flex h-14 flex-1 items-center justify-center gap-1 text-14 font-semibold text-text-3"
        >
          <Plus className="size-5 shrink-0" aria-hidden="true" />
          <span className="truncate">{t("cardPage.addToDeck")}</span>
        </button>
        {arrow(t("cardPage.next"), onNext, ChevronRight, true)}
      </div>
    </BottomBar>
  )
}
