import { Package } from "lucide-react"
import type { ReactNode } from "react"
import { useTranslation } from "react-i18next"
import { Link } from "react-router"

import { CLASS_ICON } from "../../components/card/classIcons"
import { cn } from "../../components/ui/cn"
import { classStyle } from "../../domain/classes"
import { NEUTRAL_CLASS } from "../../domain/query/model"
import { useScanClasses } from "./scan-hooks"

/** The camera screen's class picker: a column of icons along the left edge of the picture. */
export function ClassColumn({
  picked,
  onToggle,
}: {
  readonly picked: readonly string[]
  readonly onToggle: (code: string) => void
}) {
  const { t } = useTranslation()
  const classes = useScanClasses()
  if (!classes) return null
  return (
    <div role="group" aria-label={t("search.classes")} className="flex flex-col">
      {classes.map((item) => {
        const on = picked.includes(item.code)
        return (
          <button
            key={item.code}
            type="button"
            aria-pressed={on}
            aria-label={item.label}
            title={item.label}
            onClick={() => {
              onToggle(item.code)
            }}
            // A 44px target around the 36px icon (design §9 touch size).
            className="flex size-11 items-center justify-center"
          >
            <span
              className={cn(
                "flex size-9 items-center justify-center rounded-full backdrop-blur transition",
                on ? "bg-camera-chip-on ring-2 ring-camera-text" : "bg-camera-chip",
                picked.length > 0 && !on && "opacity-60",
              )}
            >
              <img
                src={CLASS_ICON[classStyle(item.code === NEUTRAL_CLASS ? null : item.code)]}
                alt=""
                className="size-5.5 object-contain"
              />
            </span>
          </button>
        )
      })}
    </div>
  )
}

function ModeTab({
  to,
  on,
  label,
}: {
  readonly to: string
  readonly on: boolean
  readonly label: string
}) {
  return on ? (
    <span
      aria-current="page"
      className="rounded-pill bg-surface-1 px-3.5 py-1.5 font-semibold text-text-1 shadow-sm"
    >
      {label}
    </span>
  ) : (
    // The pill is 30px tall; the invisible band above and below makes a 44px target.
    <Link
      to={to}
      replace
      className="relative rounded-pill px-3.5 py-1.5 text-text-2 before:absolute before:inset-x-0 before:-inset-y-2 before:content-[''] hover:text-text-1"
    >
      {label}
    </Link>
  )
}

/** Above the camera or photo on both screens: the mode switch, an optional control, the models link. */
export function ScanModeTabs({
  mode,
  start,
}: {
  readonly mode: "single" | "deck"
  readonly start?: ReactNode
}) {
  const { t } = useTranslation()
  return (
    <div className="relative mx-4 flex h-9 shrink-0 items-center justify-center lg:mx-0">
      <nav
        aria-label={t("scan.modeSwitch")}
        className="flex rounded-pill bg-surface-2 p-0.5 text-13 whitespace-nowrap"
      >
        <ModeTab to="/cards/scan" on={mode === "single"} label={t("scan.modeSingle")} />
        <ModeTab to="/cards/scan/deck" on={mode === "deck"} label={t("scan.modeDeck")} />
      </nav>
      {start !== undefined && <div className="absolute left-0">{start}</div>}
      <Link
        to="/cards/scan/models"
        className="absolute right-0 flex h-9 items-center gap-1 rounded-pill bg-surface-2 px-3 text-13 text-text-1 before:absolute before:inset-x-0 before:-inset-y-1 before:content-[''] hover:bg-surface-3"
      >
        <Package className="size-4" aria-hidden="true" /> {t("scan.models.link")}
      </Link>
    </div>
  )
}
