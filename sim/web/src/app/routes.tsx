import { redirect, type RouteObject } from "react-router"

import { SettingsPage } from "../pages/account/SettingsPage"
import { CardPage } from "../pages/card/CardPage"
import { CardsPage } from "../pages/cards/CardsPage"
import { DecksStubPage } from "../pages/decks/DecksStubPage"
import { ErrataPage } from "../pages/errata/ErrataPage"
import { HomePage } from "../pages/home/HomePage"
import { NotFoundPage } from "../pages/not-found/NotFoundPage"
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
      { path: "decks", Component: DecksStubPage },
      { path: "errata", Component: ErrataPage },
      { path: "*", Component: NotFoundPage },
    ],
  },
]
