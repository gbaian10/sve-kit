import { useTranslation } from "react-i18next"

import { cn } from "../ui/cn"
import { ATTACK_ICON, DEFENSE_ICON } from "./symbolIcons"

/**
 * The official card page's one-line stats: a green cost dot, then the attack and defense icons,
 * each followed by its name and number. Names never break; a group that does not fit wraps whole.
 */
interface StatLineProps {
  readonly cost: number | null
  readonly attack: number | null
  readonly defense: number | null
  /** `sm` for list rows: 13px numbers, 16px icons. */
  readonly size?: "md" | "sm"
  readonly className?: string
  readonly labels?: { readonly cost: string; readonly attack: string; readonly defense: string }
}

export function StatLine(props: StatLineProps) {
  return props.labels ? <Stats {...props} labels={props.labels} /> : <LocalizedStats {...props} />
}

function LocalizedStats(props: StatLineProps) {
  const { t } = useTranslation()
  return (
    <Stats
      {...props}
      labels={{
        cost: t("cardPage.stats.cost"),
        attack: t("cardPage.stats.attack"),
        defense: t("cardPage.stats.defense"),
      }}
    />
  )
}

function Stats({
  cost,
  attack,
  defense,
  size = "md",
  className,
  labels,
}: StatLineProps & { readonly labels: NonNullable<StatLineProps["labels"]> }) {
  const small = size === "sm"
  const stats = [
    { key: "cost", icon: null, label: labels.cost, value: cost, tone: "text-success" },
    {
      key: "attack",
      icon: ATTACK_ICON,
      label: labels.attack,
      value: attack,
      tone: "text-cost",
    },
    {
      key: "defense",
      icon: DEFENSE_ICON,
      label: labels.defense,
      value: defense,
      tone: "text-def",
    },
  ].filter((stat) => stat.value !== null)
  if (stats.length === 0) return null
  return (
    <p
      className={cn("flex flex-wrap items-center gap-x-3 gap-y-1 text-15 tabular-nums", className)}
    >
      {stats.map((stat) => (
        <span key={stat.key} className="inline-flex items-center gap-1 whitespace-nowrap">
          {stat.icon === null ? (
            <span
              aria-hidden="true"
              className={cn(
                "rounded-full bg-success ring-1 ring-text-1/30 ring-inset",
                small ? "size-3" : "size-3.5",
              )}
            />
          ) : (
            <img src={stat.icon} alt="" className={small ? "size-4" : "size-4.5"} />
          )}
          <span className={cn(small ? "text-12 font-bold" : "text-14 font-bold", stat.tone)}>
            {stat.label}
          </span>
          <b className="font-bold">{stat.value}</b>
        </span>
      ))}
    </p>
  )
}
