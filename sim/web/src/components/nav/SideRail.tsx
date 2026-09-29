import { useTranslation } from "react-i18next"
import { NavLink } from "react-router"

import logo from "../../assets/official/logo/head-left.png"
import { cn } from "../ui/cn"
import { NAV_ITEMS } from "./nav-items"

// Desktop / tablet-landscape navigation (design 03c, §5): a fixed 72px rail with 60×56 cells.
// Every item already carries its label, so the design's optional expanded form added nothing but
// a wordmark while narrowing the content, and the account avatar lives in the top bar like on
// every other layout; both changed on the user's call (2026-09-29). The logo is the design's
// large form (rounded square, whole artwork) at 48px.
export function SideRail() {
  const { t } = useTranslation()
  return (
    <aside className="fixed inset-y-0 left-0 z-40 hidden w-18 flex-col items-center border-r border-border bg-side py-3 lg:flex">
      <NavLink
        to="/"
        end
        aria-label={t("nav.logoAlt")}
        className="flex h-16 items-center justify-center rounded-control"
      >
        <img
          src={logo}
          alt=""
          className="size-12 rounded-control border border-border object-cover"
        />
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
