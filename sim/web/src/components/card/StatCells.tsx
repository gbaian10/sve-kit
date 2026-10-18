import { useTranslation } from "react-i18next"

import { cn } from "../ui/cn"
import { ATTACK_ICON, costIcon, costIconNeedsNumber, DEFENSE_ICON } from "./symbolIcons"

/**
 * The card head's three stat cells (design 06): 44px tall, the official cost crystal at 24px and
 * the attack and defense icons at 20px, each with its number; a missing stat prints its name and a dash.
 */
export function StatCells({
  cost,
  attack,
  defense,
  className,
}: {
  readonly cost: number | null
  readonly attack: number | null
  readonly defense: number | null
  readonly className?: string
}) {
  const { t } = useTranslation()
  if (cost === null && attack === null && defense === null) return null
  const cells = [
    {
      key: "cost",
      label: t("cardPage.stats.cost"),
      value: cost,
      icon: cost === null ? null : costIcon(cost),
      iconClass: "size-6",
      // The crystal already shows numbers 0–10; the X crystal needs the number written out.
      numberHidden: cost !== null && !costIconNeedsNumber(cost),
    },
    {
      key: "attack",
      label: t("cardPage.stats.attack"),
      value: attack,
      icon: ATTACK_ICON,
      iconClass: "size-5",
      numberHidden: false,
    },
    {
      key: "defense",
      label: t("cardPage.stats.defense"),
      value: defense,
      icon: DEFENSE_ICON,
      iconClass: "size-5",
      numberHidden: false,
    },
  ]
  const none = t("cardPage.noValue")
  return (
    <ul className={cn("grid grid-cols-3 gap-1.5", className)}>
      {cells.map((cell) => (
        <li
          key={cell.key}
          aria-label={`${cell.label} ${cell.value === null ? none : String(cell.value)}`}
          className="flex h-11 items-center justify-center gap-1 rounded-control border border-border bg-surface-1 text-16 font-bold tabular-nums"
        >
          {cell.value === null ? (
            <span className="text-12 font-medium text-text-3">
              {cell.label} {none}
            </span>
          ) : (
            <>
              {cell.icon !== null && (
                <img src={cell.icon} alt={cell.label} className={cell.iconClass} />
              )}
              <b className={cell.numberHidden ? "sr-only" : undefined}>{cell.value}</b>
            </>
          )}
        </li>
      ))}
    </ul>
  )
}
