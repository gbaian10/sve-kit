import { useTranslation } from "react-i18next"
import { Outlet, ScrollRestoration } from "react-router"

import { BottomNav } from "../components/nav/BottomNav"
import { Footer } from "../components/nav/Footer"
import { SideRail } from "../components/nav/SideRail"
import { TopBar } from "../components/nav/TopBar"
import { ToastProvider } from "../components/ui/Toast"
import { snapshotClient } from "../data"
import { showsOtherEdition, usePrefs } from "../settings"
import { CARD_PARAM } from "./card-overlay"
import { useUiLanguageSync } from "./language"
import { useOtherEdition } from "./other-edition"
import { useActiveSnapshot, useRouteEdition, useSearchReadiness } from "./snapshot"
import { SnapshotNotice } from "./SnapshotNotice"
import { useThemeAttributes } from "./theme-attributes"

// Layout switches on content width (design §11): < lg bottom bar, ≥ lg left rail (72px, 200px
// expanded from xl); content is capped at 1280 and centred.
// Switching the result view (grid / table / list) is the same list, so it keeps its scroll position;
// so is opening a card beside it.
function scrollKeySearch(search: string): string {
  const params = new URLSearchParams(search)
  params.delete("view")
  params.delete(CARD_PARAM)
  const rest = params.toString()
  return rest === "" ? "" : `?${rest}`
}

export function AppShell() {
  const { t } = useTranslation()
  useThemeAttributes()
  useUiLanguageSync()
  const { client, status, root } = useActiveSnapshot()
  const edition = useRouteEdition()
  const prefs = usePrefs()
  const readiness = useSearchReadiness(edition)
  useOtherEdition(
    snapshotClient(root),
    edition,
    showsOtherEdition(prefs, edition),
    readiness.phase === "ready" && readiness.edition === edition && !readiness.updating,
    readiness.manifestHash,
  )
  return (
    <ToastProvider>
      {/* The card drawer is a column at the right edge: the frame pads for it like for the rail. */}
      <div
        data-app-frame=""
        className="flex min-h-dvh flex-col pr-[var(--drawer-width,0px)] pb-21 transition-[padding-right] duration-200 ease-out motion-reduce:transition-none lg:pb-0 lg:pl-18 xl:pl-50 phone-landscape:pb-0 phone-landscape:pl-16"
      >
        <a
          href="#main"
          className="sr-only focus:not-sr-only focus:fixed focus:top-2 focus:left-2 focus:z-50 focus:rounded-button focus:bg-surface-1 focus:px-4 focus:py-2"
        >
          {t("nav.skipToContent")}
        </a>
        <SideRail />
        <TopBar />
        <SnapshotNotice client={client} status={status} />
        <main id="main" className="mx-auto w-full max-w-320 flex-1 px-4 lg:px-6">
          <Outlet />
        </main>
        <Footer dataVersion={status.state === "ready" ? status.dataVersion : undefined} />
        <BottomNav />
      </div>
      <ScrollRestoration
        getKey={(location) => location.pathname + scrollKeySearch(location.search)}
      />
    </ToastProvider>
  )
}
