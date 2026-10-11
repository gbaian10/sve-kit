import { ExternalLink } from "lucide-react"
import { useId } from "react"
import { useTranslation } from "react-i18next"

import type { Region } from "../../domain/search"

// Official news is only linked (design NOTES 9): no feed, no embed, no dates of our own. The links
// follow the selected card edition, since each region's official site and account post their own news.
const OFFICIAL: Record<Region, { readonly news: string; readonly x: string }> = {
  jp: { news: "https://shadowverse-evolve.com/news/", x: "https://x.com/shadowverse_ev" },
  en: { news: "https://en.shadowverse-evolve.com/news/", x: "https://x.com/svevolve_en" },
}

function hostPath(url: string): string {
  const parsed = new URL(url)
  return `${parsed.host}${parsed.pathname.replace(/\/$/u, "")}`
}

export function OfficialNews({ edition }: { readonly edition: Region }) {
  const { t } = useTranslation()
  const headingId = useId()
  const links = [
    { href: OFFICIAL[edition].news, label: t(`home.news.site.${edition}`) },
    { href: OFFICIAL[edition].x, label: t(`home.news.x.${edition}`) },
  ]
  return (
    <section aria-labelledby={headingId} className="flex flex-col gap-2">
      <h2 id={headingId} className="flex min-h-8 items-center text-15 font-semibold">
        <span>{t("home.news.title")}</span>
      </h2>
      <ul className="grid gap-2 md:grid-cols-2 lg:grid-cols-1">
        {links.map((link) => (
          <li key={link.href}>
            <a
              href={link.href}
              target="_blank"
              rel="noopener noreferrer"
              className="flex min-h-14 items-center gap-3 rounded-button border border-border bg-surface-1 px-3 py-1.5 text-text-1 hover:bg-surface-2"
            >
              <span className="block min-w-0 flex-1 leading-tight">
                <span className="block text-14 font-semibold">{link.label}</span>
                <span className="block truncate font-sans text-12 text-text-3">
                  {hostPath(link.href)}
                </span>
              </span>
              <ExternalLink className="size-4 shrink-0 text-text-3" aria-hidden="true" />
              <span className="sr-only">{t("home.news.newTab")}</span>
            </a>
          </li>
        ))}
      </ul>
      <p className="text-12 leading-relaxed text-text-3">{t("home.news.hint")}</p>
    </section>
  )
}
