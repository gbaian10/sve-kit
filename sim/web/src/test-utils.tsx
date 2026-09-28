import { render, type RenderResult } from "@testing-library/react"
import type { ReactNode } from "react"
import { I18nextProvider } from "react-i18next"
import { createMemoryRouter, type RouteObject, RouterProvider } from "react-router"

import { ToastProvider } from "./components/ui/Toast"
import { createI18n, type UiLanguage } from "./i18n"

export interface RenderRoutesOptions {
  readonly initialEntries?: readonly string[]
  readonly language?: UiLanguage
}

/** Renders a route table in a memory router with i18n, returning the router for navigation. */
export async function renderRoutes(routes: RouteObject[], options: RenderRoutesOptions = {}) {
  const i18n = await createI18n(options.language ?? "zh-TW")
  const router = createMemoryRouter(routes, {
    initialEntries: [...(options.initialEntries ?? ["/"])],
  })
  const result = render(
    <I18nextProvider i18n={i18n}>
      <RouterProvider router={router} />
    </I18nextProvider>,
  )
  return { ...result, router, i18n }
}

/** Renders a single element inside a memory router (for components that use router hooks). */
export async function renderInRouter(
  element: ReactNode,
  options: RenderRoutesOptions = {},
): Promise<RenderResult & { router: ReturnType<typeof createMemoryRouter> }> {
  const routes: RouteObject[] = [{ path: "*", element: <ToastProvider>{element}</ToastProvider> }]
  return renderRoutes(routes, options)
}
