import { useTranslation } from "react-i18next"
import { Outlet, ScrollRestoration, useLocation } from "react-router"

import { BottomNav } from "../components/nav/BottomNav"
import { Footer } from "../components/nav/Footer"
import { TopBar } from "../components/nav/TopBar"
import { cn } from "../components/ui/cn"
import { ToastProvider } from "../components/ui/Toast"
import { snapshotClient } from "../data"
import { showsOtherEdition, usePrefs } from "../settings"
import { CARD_PARAM } from "./card-overlay"
import { useUiLanguageSync } from "./language"
import { useOtherEdition } from "./other-edition"
import { useActiveSnapshot, useRouteEdition, useSearchReadiness } from "./snapshot"
import { SnapshotNotice } from "./SnapshotNotice"
import { useThemeAttributes } from "./theme-attributes"

// Layout switches on content width (design §11): < lg bottom bar, ≥ lg the links join the top bar;
// content is capped at 1280 and centred, except the cards list, which fills the window.
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
  // The cards list is not centred or capped, so its filter sidebar sits at the window edge.
  const fullWidth = useLocation().pathname === "/cards"
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
      {/* The card drawer is a column at one edge: the frame pads that side for it. */}
      <div
        data-app-frame=""
        className="flex min-h-dvh flex-col overflow-x-clip pr-[calc(var(--drawer-width,0px)*var(--drawer-on-right,0))] pb-21 pl-[calc(var(--drawer-width,0px)*var(--drawer-on-left,0))] transition-[padding] duration-200 ease-out motion-reduce:transition-none lg:pb-0 phone-landscape:pb-0 phone-landscape:pl-16"
      >
        <a
          href="#main"
          className="sr-only focus:not-sr-only focus:fixed focus:top-2 focus:left-2 focus:z-50 focus:rounded-button focus:bg-surface-1 focus:px-4 focus:py-2"
        >
          {t("nav.skipToContent")}
        </a>
        <TopBar />
        <SnapshotNotice client={client} status={status} />
        <main
          id="main"
          className={cn("w-full flex-1 px-4", fullWidth ? "lg:px-3" : "mx-auto max-w-320 lg:px-6")}
        >
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
