import { type ReactNode, type RefObject, useEffect, useMemo, useRef } from "react"
import { useTranslation } from "react-i18next"
import { useLocation, useNavigate, useSearchParams } from "react-router"

import {
  CARD_PARAM,
  cardParamOf,
  type CardPresentation,
  parseCardParam,
  useCardPresentation,
  withoutCardParam,
} from "../../app/card-overlay"
import {
  type CardEntryState,
  type ListEntryState,
  type OverlayEntryState,
  routeState,
} from "../../app/listEntryState"
import { useRouteEdition } from "../../app/snapshot"
import { Dialog } from "../../components/ui/Dialog"
import type { CardRouteInput, CardSummary } from "../../data"
import { printingPath } from "../../domain/cardRoute"
import type { CardFrame, OverlayLinkTarget } from "../card/CardPage"
import { routeInput } from "../card/useCardContent"
import { CardOverlayContent } from "./CardOverlayContent"

export interface CardOverlayArgs {
  readonly entry: ListEntryState
  /** The list's search string without the card, which previous/next walk. */
  readonly background: string
  readonly pages: number
  readonly listScroll: RefObject<{ readonly top: number; readonly left: number }>
  readonly root: RefObject<HTMLElement | null>
}

export interface CardOverlay {
  readonly presentation: CardPresentation
  /** Whether a card is shown beside or over the results. */
  readonly open: boolean
  /** The result cell whose card is open, when the list knows it. */
  readonly selected: string | undefined
  /** Opens the cell's card in place; false when cards open as pages, so the caller navigates. */
  readonly openInPlace: (summary: CardSummary, key: string) => boolean
  readonly close: () => void
  /** The side panel for the results' right column, or null. */
  readonly panel: ReactNode
  /** The modal over the results; always rendered so its dialog can hand focus back on close. */
  readonly modal: ReactNode
}

const PANEL_CLASS =
  "mt-3 rounded-block border border-border bg-surface-1 outline-none lg:sticky lg:top-4 lg:mt-0 lg:overflow-y-auto"
// The panel sticks 1rem below the viewport top; its height follows its own top edge until then.
const PANEL_GAP_PX = 16

/**
 * The card a list shows beside (panel) or over (modal) its results: `?card=` names it, the list's
 * history entry keeps what the opening cell knew (`overlay`), and the preference decides the frame.
 */
export function useCardOverlay({
  entry,
  background,
  pages,
  listScroll,
  root,
}: CardOverlayArgs): CardOverlay {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const location = useLocation()
  const [params] = useSearchParams()
  const edition = useRouteEdition()
  const presentation = useCardPresentation()
  const param = params.get(CARD_PARAM)
  const target = useMemo(() => parseCardParam(param), [param])
  const overlay = entry.overlay
  const open = target !== null && presentation !== "page"

  // The preference says "page" (or changed to it while open): the card URL becomes the page.
  useEffect(() => {
    if (!target || presentation !== "page") return
    const path =
      target.namespace === "provisional"
        ? printingPath({ cardNo: "", cardNoState: "provisional", intId: Number(target.key) })
        : printingPath({ cardNo: target.key })
    const state: CardEntryState | null = overlay?.resultKey
      ? {
          background,
          source: "results",
          pages,
          resultKey: overlay.resultKey,
          ...(overlay.route ? { route: overlay.route } : {}),
        }
      : null
    void navigate(path, { replace: true, state })
  }, [target, presentation, overlay, background, pages, navigate])

  const openInPlace = (summary: CardSummary, key: string) => {
    if (presentation === "page") return false
    const next = new URLSearchParams(location.search)
    const alreadyOpen = next.has(CARD_PARAM)
    next.set(CARD_PARAM, cardParamOf(summary))
    const { overlay: previous, ...list } = entry
    const state: ListEntryState = {
      ...list,
      pages,
      listScroll: listScroll.current,
      scrollY: window.scrollY,
      overlay: {
        resultKey: key,
        ...routeState(summary),
        pushed: alreadyOpen ? (previous?.pushed ?? false) : true,
      },
    }
    void navigate(
      { pathname: location.pathname, search: `?${next.toString()}` },
      { replace: alreadyOpen, preventScrollReset: true, state },
    )
    return true
  }

  const close = () => {
    if (overlay?.pushed) {
      void navigate(-1)
      return
    }
    const { overlay: _closed, ...list } = entry
    void navigate(
      { pathname: location.pathname, search: withoutCardParam(location.search) },
      { replace: true, preventScrollReset: true, state: list },
    )
  }

  // Focus enters the panel when it opens and returns to the opening cell when it closes, however
  // it closed; the modal's dialog does the same on its own.
  const panelRef = useRef<HTMLElement>(null)
  const lastSelected = useRef<string | null>(null)
  const selected = open ? overlay?.resultKey : undefined
  useEffect(() => {
    if (open) {
      if (lastSelected.current === null && presentation === "panel") panelRef.current?.focus()
      lastSelected.current = selected ?? ""
      return
    }
    const key = lastSelected.current
    lastSelected.current = null
    if (key === null || key === "") return
    const cell = [
      ...(root.current?.querySelectorAll<HTMLElement>("a[data-result-key]") ?? []),
    ].find((link) => link.getAttribute("data-result-key") === key)
    cell?.focus()
  }, [open, selected, presentation, root])

  const panelOpen = open && presentation === "panel"
  useEffect(() => {
    const panel = panelRef.current
    if (!panelOpen || !panel) return
    const fit = () => {
      const top = Math.max(panel.getBoundingClientRect().top, PANEL_GAP_PX)
      panel.style.maxHeight = `calc(100dvh - ${String(Math.round(top) + PANEL_GAP_PX)}px)`
    }
    fit()
    window.addEventListener("scroll", fit, { passive: true })
    window.addEventListener("resize", fit)
    return () => {
      window.removeEventListener("scroll", fit)
      window.removeEventListener("resize", fit)
      panel.style.maxHeight = ""
    }
  }, [panelOpen])

  const closeOnEscape = panelOpen
  useEffect(() => {
    if (!closeOnEscape) return
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || event.defaultPrevented) return
      // Dialogs (zoom, filters) and the suggestion list take Escape first.
      if (event.target instanceof Element && event.target.closest("dialog, [role=listbox]")) return
      event.preventDefault()
      close()
    }
    window.addEventListener("keydown", onKeyDown)
    return () => {
      window.removeEventListener("keydown", onKeyDown)
    }
  })

  const routeKey = JSON.stringify(overlay?.route ?? null)
  const input = useMemo<CardRouteInput | null>(
    () =>
      target
        ? routeInput(
            target.namespace,
            target.key,
            edition,
            (JSON.parse(routeKey) as OverlayEntryState["route"] | null) ?? undefined,
          )
        : null,
    [target, edition, routeKey],
  )
  const link = (to: OverlayLinkTarget) => {
    const next = new URLSearchParams(location.search)
    next.set(CARD_PARAM, cardParamOf(to))
    const { overlay: previous, ...list } = entry
    const state: ListEntryState = {
      ...list,
      overlay: {
        ...(to.resultKey ? { resultKey: to.resultKey } : {}),
        ...(to.route ? { route: to.route } : {}),
        ...(previous?.pushed ? { pushed: true } : {}),
      },
    }
    return { to: { pathname: location.pathname, search: `?${next.toString()}` }, state }
  }
  const frame: CardFrame | null = target
    ? {
        entry: {
          background,
          source: "results",
          pages,
          ...(overlay?.resultKey ? { resultKey: overlay.resultKey } : {}),
          ...(overlay?.route ? { route: overlay.route } : {}),
        },
        key: target.key,
        link,
        onClose: close,
        layout: presentation === "panel" ? "column" : "wide",
      }
    : null
  const content = input && frame ? <CardOverlayContent input={input} frame={frame} /> : null

  return {
    presentation,
    open,
    selected,
    openInPlace,
    close,
    panel:
      open && presentation === "panel" ? (
        <aside
          ref={panelRef}
          tabIndex={-1}
          data-card-overlay=""
          aria-label={t("cardPage.panel")}
          className={PANEL_CLASS}
        >
          {content}
        </aside>
      ) : null,
    modal: (
      <Dialog
        open={open && presentation === "modal"}
        onClose={close}
        title={t("cardPage.panel")}
        variant="card"
        bare
      >
        {open && presentation === "modal" && (
          <div data-card-overlay="" className="min-h-full">
            {content}
          </div>
        )}
      </Dialog>
    ),
  }
}
