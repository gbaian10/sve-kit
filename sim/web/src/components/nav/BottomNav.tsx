import { useTranslation } from "react-i18next"
import { NavLink } from "react-router"

import { cn } from "../ui/cn"
import { BottomBar } from "./BottomBar"
import { NAV_ITEMS } from "./nav-items"

// Phone / tablet-portrait navigation (design 03a/03b): three 84px cells (62 + 22 safe area),
// 22px icon over an 11px label, selected = 56×28 pill in accent-soft. Landscape phones move the
// same items to a 64px rail on the left (design §11) so the short viewport keeps its height.
export function BottomNav() {
  const { t } = useTranslation()
  return (
    <BottomBar className="lg:hidden phone-landscape:inset-y-0 phone-landscape:right-auto phone-landscape:w-16 phone-landscape:border-t-0 phone-landscape:border-r phone-landscape:pb-0">
      <nav aria-label={t("nav.primary")} className="phone-landscape:h-full">
        <ul className="flex h-15.5 items-stretch phone-landscape:h-full phone-landscape:flex-col phone-landscape:justify-center">
          {NAV_ITEMS.map(({ key, to, Icon, end }) => (
            <li key={key} className="flex flex-1 phone-landscape:flex-none">
              <NavLink
                to={to}
                end={end}
                className="flex min-h-11 flex-1 flex-col items-center justify-center gap-0.5 text-11 text-text-2 phone-landscape:h-14"
              >
                {({ isActive }) => (
                  <>
                    <span
                      className={cn(
                        "flex h-7 w-14 items-center justify-center rounded-pill",
                        isActive && "bg-accent-soft text-accent-text",
                      )}
                    >
                      <Icon className="size-5.5" aria-hidden="true" />
                    </span>
                    <span className={cn(isActive && "font-semibold text-accent-text")}>
                      {t(`nav.${key}`)}
                    </span>
                  </>
                )}
              </NavLink>
            </li>
          ))}
        </ul>
      </nav>
    </BottomBar>
  )
}
