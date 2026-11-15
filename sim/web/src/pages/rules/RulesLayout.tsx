import { useTranslation } from "react-i18next"
import { NavLink, Outlet, useMatch } from "react-router"

import { cn } from "../../components/ui/cn"

const OFFICIAL_RULES_URL = "https://shadowverse-evolve.com/rules/"

// The two sections are separate URLs so each can be shared and the old /errata link can land on
// its own tab; plain links with aria-current keep that history behaviour honest (not ARIA tabs).
const SECTIONS = [
  { to: "/rules", end: true, titleKey: "rules.comprehensive" },
  { to: "/rules/errata", end: true, titleKey: "errata.heading" },
] as const

export function RulesLayout() {
  const { t } = useTranslation()
  const onErrata = useMatch("/rules/errata") !== null
  return (
    <section className="mx-auto max-w-6xl py-8 lg:py-12" aria-labelledby="rules-heading">
      <header className="mb-6">
        <p className="mb-2 text-12 font-semibold tracking-[0.16em] text-accent-text uppercase">
          {t("nav.rules")}
        </p>
        <h1 id="rules-heading" className="text-24 font-bold tracking-tight text-text-1">
          {onErrata ? t("errata.listTitle") : t("rules.comprehensive")}
        </h1>
        {!onErrata && (
          <p className="mt-2 text-13 text-text-2">
            <a
              href={OFFICIAL_RULES_URL}
              target="_blank"
              rel="noreferrer"
              className="text-accent-text underline-offset-2 hover:underline"
            >
              {t("rules.officialLink")}
            </a>
          </p>
        )}
        <nav aria-label={t("rules.sections")} className="mt-5 flex gap-1 border-b border-border">
          {SECTIONS.map((section) => (
            <NavLink
              key={section.to}
              to={section.to}
              end={section.end}
              className={({ isActive }) =>
                cn(
                  "-mb-px flex min-h-11 items-center border-b-2 px-3 text-14 font-semibold",
                  isActive
                    ? "border-text-1 text-text-1"
                    : "border-transparent text-text-2 hover:text-text-1",
                )
              }
            >
              {t(section.titleKey)}
            </NavLink>
          ))}
        </nav>
      </header>
      <Outlet />
    </section>
  )
}
