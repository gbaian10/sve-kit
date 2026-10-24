import { useTranslation } from "react-i18next"
import { NavLink } from "react-router"

import { useRouteEdition } from "../../app/snapshot"
import logo from "../../assets/official/logo/head-left.png"
import { editionUrl } from "../../domain/query/codec"
import { cn } from "../ui/cn"
import { AccountMenuButton } from "./AccountMenu"
import { NAV_ITEMS } from "./nav-items"

// Top bar: logo, the three site links (from lg; below it the bottom bar carries them) and the
// account avatar at the right end, at every size.
export function TopBar() {
  const { t } = useTranslation()
  const edition = useRouteEdition()
  return (
    <header className="@container/topbar sticky top-0 z-30 flex h-13 items-center justify-between gap-4 border-b border-border bg-side px-4 lg:px-6">
      <div className="flex items-center gap-6">
        <NavLink to="/" end aria-label={t("nav.logoAlt")} className="flex items-center gap-2">
          <img
            src={logo}
            alt=""
            className="size-8 rounded-card border border-border object-cover"
          />
          <span className="text-16 font-bold lg:@max-[480px]/topbar:sr-only">{t("app.title")}</span>
        </NavLink>
        <nav aria-label={t("nav.primary")} className="hidden lg:block">
          <ul className="flex items-center gap-1">
            {NAV_ITEMS.map(({ key, to, Icon, end }) => (
              <li key={key}>
                <NavLink
                  to={to === "/cards" || to === "/sets" ? editionUrl(to, edition) : to}
                  end={end}
                  className={({ isActive }) =>
                    cn(
                      "flex h-9 items-center gap-2 rounded-button px-3 text-14 text-text-2 hover:bg-surface-2",
                      isActive && "bg-accent-soft font-semibold text-accent-text",
                    )
                  }
                >
                  <Icon className="size-4.5 shrink-0" aria-hidden="true" />
                  <span className="@max-[480px]/topbar:sr-only">{t(`nav.${key}`)}</span>
                </NavLink>
              </li>
            ))}
          </ul>
        </nav>
      </div>
      <AccountMenuButton />
    </header>
  )
}
