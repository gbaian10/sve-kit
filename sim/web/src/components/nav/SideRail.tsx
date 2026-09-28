import { ChevronLeft, ChevronRight } from "lucide-react"
import { useTranslation } from "react-i18next"
import { NavLink } from "react-router"

import logo from "../../assets/official/logo/head-left.png"
import { cn } from "../ui/cn"
import { AccountMenuButton } from "./AccountMenu"
import { NAV_ITEMS } from "./nav-items"

export interface SideRailProps {
  readonly expanded: boolean
  readonly onToggle: () => void
}

// Desktop / tablet-landscape navigation (design 03c, §5): 72px rail with a 36px round logo,
// 60×56 cells, the account avatar at the bottom. At 1440 it can expand to icon + label.
export function SideRail({ expanded, onToggle }: SideRailProps) {
  const { t } = useTranslation()
  return (
    <aside
      className={cn(
        "fixed inset-y-0 left-0 z-40 hidden flex-col items-center border-r border-border bg-side py-3 lg:flex",
        expanded ? "w-18 xl:w-50" : "w-18",
      )}
    >
      <NavLink to="/" end className="flex h-14 items-center justify-center">
        <img src={logo} alt={t("nav.logoAlt")} className="size-9 rounded-full ring-2 ring-text-1" />
      </NavLink>
      <nav aria-label={t("nav.primary")} className="mt-2 w-full">
        <ul className="flex flex-col items-stretch gap-1 px-1.5">
          {NAV_ITEMS.map(({ key, to, Icon, end }) => (
            <li key={key}>
              <NavLink
                to={to}
                end={end}
                className={({ isActive }) =>
                  cn(
                    "flex h-14 flex-col items-center justify-center gap-0.5 rounded-button text-11 text-text-2 hover:bg-surface-2",
                    expanded && "xl:flex-row xl:justify-start xl:gap-3 xl:px-4 xl:text-14",
                    isActive && "bg-accent-soft font-semibold text-accent-text",
                  )
                }
              >
                <Icon className="size-5.5 shrink-0" aria-hidden="true" />
                <span>{t(`nav.${key}`)}</span>
              </NavLink>
            </li>
          ))}
        </ul>
      </nav>
      <div className="mt-auto flex flex-col items-center gap-2">
        <button
          type="button"
          onClick={onToggle}
          aria-expanded={expanded}
          aria-label={expanded ? t("nav.collapse") : t("nav.expand")}
          className="hidden size-11 items-center justify-center rounded-button text-text-2 hover:bg-surface-2 xl:flex"
        >
          {expanded ? (
            <ChevronLeft className="size-5" aria-hidden="true" />
          ) : (
            <ChevronRight className="size-5" aria-hidden="true" />
          )}
        </button>
        <AccountMenuButton placement="rail" />
      </div>
    </aside>
  )
}
