import { useTranslation } from "react-i18next"
import { NavLink } from "react-router"

import logo from "../../assets/official/logo/head-left.png"
import { AccountMenuButton } from "./AccountMenu"

// Phone / tablet-portrait top bar: logo + wordmark on the left (hidden on phones so page headers
// get the width, design §5), avatar on the right. Desktop pages own their own top rows.
export function TopBar() {
  const { t } = useTranslation()
  return (
    <header className="flex h-13 items-center justify-between px-4 lg:justify-end lg:px-6">
      <NavLink to="/" end className="hidden items-center gap-2 md:flex lg:hidden">
        <img src={logo} alt="" className="size-8 rounded-full ring-2 ring-text-1" />
        <span className="text-16 font-bold">{t("app.title")}</span>
      </NavLink>
      <span className="text-16 font-bold md:hidden">{t("app.title")}</span>
      <div className="lg:hidden">
        <AccountMenuButton placement="top" />
      </div>
    </header>
  )
}
