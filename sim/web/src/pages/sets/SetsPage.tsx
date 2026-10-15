import { useTranslation } from "react-i18next"

import { useRouteEdition, useSearchReadiness, useSetOptions } from "../../app/snapshot"
import { StatusStrip } from "../../app/StatusStrip"
import { Link } from "../../components/ui/Link"
import { UI_TEXT_LANG } from "../../domain/nameDisplay"
import { editionUrl } from "../../domain/query/codec"
import { currentUiLanguage } from "../../i18n"
import { usePrefs } from "../../settings"
import { PageTitle } from "../PageTitle"

const OTHER_KINDS = ["deck", "special", "other"]
const MAIN_KINDS = ["booster", "collaboration", "special_pack", "promo"] as const

export function SetsPage() {
  const { t } = useTranslation()
  const edition = useRouteEdition()
  const readiness = useSearchReadiness(edition)
  const { sets } = useSetOptions()
  const { uiLanguage } = usePrefs()
  const language = UI_TEXT_LANG[currentUiLanguage(uiLanguage)]
  const list = (kinds: readonly string[]) => (
    <ul className="grid gap-3 md:grid-cols-2 lg:grid-cols-3">
      {sets
        ?.filter((set) => kinds.includes(set.kind ?? "other"))
        .map((set) => (
          <li key={set.id}>
            <Link
              className="flex min-h-14 items-center gap-3 rounded-button border border-border bg-surface-1 p-3"
              to={editionUrl(`/sets/${encodeURIComponent(set.publicCode ?? set.code)}`, edition)}
            >
              <span className="font-semibold">{set.publicCode ?? set.code}</span>
              <span>{set.name.translations[language] ?? set.name.original.text}</span>
            </Link>
          </li>
        ))}
    </ul>
  )
  return (
    <div className="pb-6">
      <PageTitle>{t("pages.sets")}</PageTitle>
      <StatusStrip readiness={readiness} />
      {sets &&
        MAIN_KINDS.map((kind) => (
          <section key={kind} className="mt-5">
            <h2 className="mb-3 text-16 font-semibold">{t(`sets.kind.${kind}`)}</h2>
            {list([kind])}
          </section>
        ))}
      {sets && (
        <details className="mt-5">
          <summary className="min-h-11 cursor-pointer py-3 font-semibold">
            {t("sets.specialProducts")}
          </summary>
          {list(OTHER_KINDS)}
        </details>
      )}
    </div>
  )
}
