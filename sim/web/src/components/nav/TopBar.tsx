import { useTranslation } from "react-i18next"
import { NavLink } from "react-router"

import logo from "../../assets/official/logo/head-left.png"
import { AccountMenuButton } from "./AccountMenu"

// Top bar: small logo + wordmark on the left (below lg; the rail carries the logo on desktop) and
// the avatar on the right at every size, so the account menu is always in the same corner.
export function TopBar() {
  const { t } = useTranslation()
  return (
    <header className="flex h-13 items-center justify-between px-4 lg:justify-end lg:px-6">
      <NavLink to="/" end className="flex items-center gap-2 lg:hidden">
        <img src={logo} alt="" className="size-8 rounded-card border border-border object-cover" />
        <span className="text-16 font-bold">{t("app.title")}</span>
      </NavLink>
      <AccountMenuButton />
    </header>
  )
}
