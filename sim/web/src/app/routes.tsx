import { redirect, type RouteObject } from "react-router"

import { SettingsPage } from "../pages/account/SettingsPage"
import { CardPage } from "../pages/card/CardPage"
import { CardsPage } from "../pages/cards/CardsPage"
import { DeckBuilderPage } from "../pages/decks/DeckBuilderPage"
import { DeckImportPage } from "../pages/decks/DeckImportPage"
import { DecksPage } from "../pages/decks/DecksPage"
import { HomePage } from "../pages/home/HomePage"
import { NotFoundPage } from "../pages/not-found/NotFoundPage"
import { ComprehensiveRulesPage } from "../pages/rules/ComprehensiveRulesPage"
import { ErrataPage } from "../pages/rules/ErrataPage"
import { ErrataRedirect } from "../pages/rules/ErrataRedirect"
import { RulesLayout } from "../pages/rules/RulesLayout"
import { DeckScanPage } from "../pages/scan/DeckScanPage"
import { ScanModelsPage } from "../pages/scan/ScanModelsPage"
import { ScanPage } from "../pages/scan/ScanPage"
import { SetsPage } from "../pages/sets/SetsPage"
import { AppShell } from "./AppShell"

// Route table from docs/sim/web-architecture.md §2. Card is a sibling of Cards on purpose: the
// overlay/full-page choice is made from location.state, not from nesting.
export const routes: RouteObject[] = [
  {
    path: "/",
    Component: AppShell,
    children: [
      { index: true, Component: HomePage },
      { path: "cards", Component: CardsPage },
      { path: "cards/_provisional/:intId", Component: CardPage },
      { path: "cards/unimplemented", Component: NotFoundPage },
      { path: "cards/scan", Component: ScanPage },
      { path: "cards/scan/deck", Component: DeckScanPage },
      { path: "cards/scan/models", Component: ScanModelsPage },
      { path: "cards/:cardNo/:slug?", Component: CardPage },
      { path: "sets", Component: SetsPage },
      {
        path: "sets/:code",
        loader: ({ params, request }) => {
          const url = new URL(request.url)
          url.searchParams.set("set", (params["code"] ?? "").toLowerCase())
          return redirect(`/cards${url.search}${url.hash}`)
        },
      },
      { path: "settings", Component: SettingsPage },
      { path: "decks", Component: DecksPage },
      { path: "decks/import", Component: DeckImportPage },
      { path: "decks/:deckId", Component: DeckBuilderPage },
      {
        path: "rules",
        Component: RulesLayout,
        children: [
          { index: true, Component: ComprehensiveRulesPage },
          { path: "errata", Component: ErrataPage },
        ],
      },
      { path: "errata", Component: ErrataRedirect },
      { path: "*", Component: NotFoundPage },
    ],
  },
  // The battle screen fills the window: it is not a page inside the shell. It exists in dev builds
  // only; the variable is read here, in place, so that a build without it drops the page's code.
  ...(import.meta.env.VITE_DEV_ONLY_PLAY === "1"
    ? [
        {
          path: "/play",
          lazy: async () => ({ Component: (await import("../pages/play/PlayPage")).PlayPage }),
        },
      ]
    : []),
]
