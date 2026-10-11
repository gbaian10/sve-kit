import { useTranslation } from "react-i18next"
import { NavLink } from "react-router"

import logo from "../../assets/official/logo/head-left.png"
import { cn } from "../ui/cn"
import { NAV_ITEMS } from "./nav-items"

// Keep the account avatar in the top bar across layouts (user decision 2026-09-29).
export function SideRail() {
  const { t } = useTranslation()
  return (
    <aside className="fixed inset-y-0 left-0 z-40 hidden w-18 flex-col border-r border-border bg-side py-3 lg:flex xl:w-50">
      <NavLink
        to="/"
        end
        aria-label={t("nav.logoAlt")}
        className="mx-auto flex h-16 items-center gap-3 rounded-control px-1.5 xl:mx-3 xl:px-2"
      >
        <img
          src={logo}
          alt=""
          className="size-12 shrink-0 rounded-control border border-border object-cover xl:size-9"
        />
        <span className="hidden text-16 font-bold xl:inline">{t("app.title")}</span>
      </NavLink>
      <nav aria-label={t("nav.primary")} className="mt-2 w-full">
        <ul className="flex flex-col items-stretch gap-1 px-1.5 xl:px-3">
          {NAV_ITEMS.map(({ key, to, Icon, end }) => (
            <li key={key}>
              <NavLink
                to={to}
                end={end}
                className={({ isActive }) =>
                  cn(
                    "flex h-14 flex-col items-center justify-center gap-0.5 rounded-button text-11 text-text-2 hover:bg-surface-2",
                    "xl:h-11 xl:flex-row xl:justify-start xl:gap-3 xl:px-3 xl:text-15",
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
    </aside>
  )
}
