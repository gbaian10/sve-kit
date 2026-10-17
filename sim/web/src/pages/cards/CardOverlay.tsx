import { type ReactNode, type RefObject, useEffect, useMemo, useRef, useState } from "react"
import { useTranslation } from "react-i18next"
import { useLocation, useNavigate, useSearchParams } from "react-router"

import {
  CARD_PARAM,
  cardParamOf,
  type CardPresentation,
  DRAWER_WIDTH_VAR,
  effectiveDrawerWidth,
  parseCardParam,
  useCardPresentation,
  useViewportWidth,
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
import type { QueryState } from "../../domain/query/model"
import { usePrefs } from "../../settings"
import type { CardFrame, OverlayLinkTarget } from "../card/CardPage"
import { routeInput } from "../card/useCardContent"
import { CardOverlayContent } from "./CardOverlayContent"
import { DrawerHandle } from "./DrawerHandle"

export interface CardOverlayArgs {
  readonly entry: ListEntryState
  /** The list's search string without the card, which previous/next walk. */
  readonly background: string
  readonly pages: number
  /** What one result cell stands for; decides which cell another printing of the card belongs to. */
  readonly unit: QueryState["unit"]
  readonly listScroll: RefObject<{ readonly top: number; readonly left: number }>
  readonly root: RefObject<HTMLElement | null>
}

export interface CardOverlay {
  readonly presentation: CardPresentation
  /** Whether a card is shown beside or over the results. */
  readonly open: boolean
  /** The result cell whose card is open, when the list knows it. */
  readonly selected: string | undefined
  /** The cell the last closed card returns to, highlighted like a cell returned to from the page. */
  readonly returned: string | undefined
  /** Opens the cell's card in place; false when cards open as pages, so the caller navigates. */
  readonly openInPlace: (summary: CardSummary, key: string) => boolean
  readonly close: () => void
  /** The drawer at the app frame's right edge, kept while it slides out; null otherwise. */
  readonly drawer: ReactNode
  /** The modal over the results; always rendered so its dialog can hand focus back on close. */
  readonly modal: ReactNode
}

// A column of the app frame like the side rail (same layer); the frame pads its right side by the
// drawer width while it is open, so the results reflow instead of being covered.
const DRAWER_CLASS =
  "fixed inset-y-0 right-0 z-40 flex border-l border-border bg-surface-1 shadow-lg outline-none transition-transform duration-200 ease-out starting:translate-x-full data-[state=closing]:translate-x-full motion-reduce:transition-none"
const EXIT_MS = 200

function cellOf(root: HTMLElement | null, key: string | undefined): HTMLElement | undefined {
  if (key === undefined || key === "") return undefined
  return [...(root?.querySelectorAll<HTMLElement>("a[data-result-key]") ?? [])].find(
    (link) => link.getAttribute("data-result-key") === key,
  )
}

/**
 * The card a list shows beside (drawer) or over (modal) its results: `?card=` names it, the list's
 * history entry keeps what the opening cell knew (`overlay`), and the preference decides the frame.
 */
export function useCardOverlay({
  entry,
  background,
  pages,
  unit,
  listScroll,
  root,
}: CardOverlayArgs): CardOverlay {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const location = useLocation()
  const [params] = useSearchParams()
  const edition = useRouteEdition()
  const presentation = useCardPresentation()
  const { drawerWidth } = usePrefs()
  const viewportWidth = useViewportWidth()
  const width = effectiveDrawerWidth(drawerWidth, viewportWidth)
  const param = params.get(CARD_PARAM)
  const overlay = entry.overlay
  const open = param !== null && presentation !== "page"
  const drawerOpen = open && presentation === "panel"

  // The drawer slides out before it unmounts, still showing the card it had.
  const [wasDrawerOpen, setWasDrawerOpen] = useState(drawerOpen)
  const [closing, setClosing] = useState(false)
  if (drawerOpen !== wasDrawerOpen) {
    setWasDrawerOpen(drawerOpen)
    setClosing(!drawerOpen)
  }
  const [shownParam, setShownParam] = useState(param)
  if (param !== null && param !== shownParam) setShownParam(param)
  useEffect(() => {
    if (!closing) return
    const reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches
    const timer = setTimeout(
      () => {
        setClosing(false)
      },
      reduced ? 0 : EXIT_MS,
    )
    return () => {
      clearTimeout(timer)
    }
  }, [closing])
  const drawerShown = drawerOpen || closing
  const contentParam = open ? param : shownParam
  const target = useMemo(() => parseCardParam(contentParam), [contentParam])

  // One value drives both the drawer and the frame's padding, so they never disagree.
  useEffect(() => {
    if (!drawerOpen) return
    const style = document.documentElement.style
    style.setProperty(DRAWER_WIDTH_VAR, `${String(width)}px`)
    return () => {
      style.removeProperty(DRAWER_WIDTH_VAR)
    }
  }, [drawerOpen, width])

  // The preference says "page" (or changed to it while open): the card URL becomes the page.
  useEffect(() => {
    if (param === null || presentation !== "page") return
    const page = parseCardParam(param)
    if (!page) return
    const path =
      page.namespace === "provisional"
        ? printingPath({ cardNo: "", cardNoState: "provisional", intId: Number(page.key) })
        : printingPath({ cardNo: page.key })
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
  }, [param, presentation, overlay, background, pages, navigate])

  const openInPlace = (summary: CardSummary, key: string) => {
    if (presentation === "page") return false
    const next = new URLSearchParams(location.search)
    const alreadyOpen = next.has(CARD_PARAM)
    next.set(CARD_PARAM, cardParamOf(summary))
    const { overlay: previous, ...list } = entry
    const top = cellOf(root.current, key)?.getBoundingClientRect().top
    const state: ListEntryState = {
      ...list,
      pages,
      listScroll: listScroll.current,
      scrollY: window.scrollY,
      overlay: {
        resultKey: key,
        ...routeState(summary),
        pushed: alreadyOpen ? (previous?.pushed ?? false) : true,
        opener: key,
        ...(top === undefined ? {} : { openerTop: Math.round(top) }),
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

  // Focus enters the drawer when it opens and returns to the opening cell when it closes, however
  // it closed. The frame's padding takes a moment to give the width back, so the cell is put back
  // where it was only after that; the modal's dialog hands focus back on its own.
  const drawerRef = useRef<HTMLElement>(null)
  const opened = useRef<{ opener: string; top: number | undefined } | null>(null)
  const [returned, setReturned] = useState<string | undefined>(undefined)
  const selected = open ? overlay?.resultKey : undefined
  const opener = open ? (overlay?.opener ?? overlay?.resultKey ?? "") : undefined
  const openerTop = overlay?.openerTop
  useEffect(() => {
    if (opener !== undefined) {
      if (opened.current === null && presentation === "panel") drawerRef.current?.focus()
      opened.current = { opener, top: openerTop }
      return
    }
    const last = opened.current
    opened.current = null
    if (last === null) return
    const cell = cellOf(root.current, last.opener)
    if (!cell) return
    cell.focus({ preventScroll: true })
    const reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches
    // Rows far from the viewport are laid out from estimates (content-visibility), so one scroll
    // rarely lands exactly: scroll, let the rows near the cell render, and measure again.
    let attempts = 0
    let timer: ReturnType<typeof setTimeout>
    const settle = () => {
      const delta = last.top === undefined ? 0 : cell.getBoundingClientRect().top - last.top
      if (Math.abs(delta) > 1 && attempts < 8) {
        attempts += 1
        window.scrollBy({ top: delta })
        timer = setTimeout(settle, 16)
        return
      }
      setReturned(last.opener)
    }
    timer = setTimeout(settle, reduced ? 0 : EXIT_MS + 50)
    return () => {
      clearTimeout(timer)
    }
  }, [opener, openerTop, presentation, root])

  useEffect(() => {
    if (!drawerOpen) return
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
  // Another printing of the same card keeps the card's own result cell when cells are cards;
  // otherwise the printing stands in, and previous/next find it if it is in the loaded results.
  const link = (to: OverlayLinkTarget) => {
    const next = new URLSearchParams(location.search)
    next.set(CARD_PARAM, cardParamOf(to))
    const { overlay: previous, ...list } = entry
    const sameCard =
      unit === "card" &&
      to.route?.cardId !== undefined &&
      to.route.cardId === previous?.route?.cardId
    const kept = sameCard ? previous.resultKey : undefined
    const resultKey = to.resultKey ?? kept ?? to.route?.printingId
    const state: ListEntryState = {
      ...list,
      overlay: {
        ...(resultKey === undefined ? {} : { resultKey }),
        ...(to.route ? { route: to.route } : {}),
        ...(previous?.pushed ? { pushed: true } : {}),
        ...(previous?.opener === undefined ? {} : { opener: previous.opener }),
        ...(previous?.openerTop === undefined ? {} : { openerTop: previous.openerTop }),
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
      }
    : null
  const content = input && frame ? <CardOverlayContent input={input} frame={frame} /> : null

  return {
    presentation,
    open,
    selected,
    returned: open ? undefined : returned,
    openInPlace,
    close,
    drawer: drawerShown ? (
      <aside
        ref={drawerRef}
        tabIndex={-1}
        data-card-overlay=""
        data-state={drawerOpen ? "open" : "closing"}
        aria-label={t("cardPage.panel")}
        aria-hidden={drawerOpen ? undefined : true}
        inert={!drawerOpen}
        className={DRAWER_CLASS}
        style={{ width: `var(${DRAWER_WIDTH_VAR}, ${String(width)}px)` }}
      >
        <DrawerHandle />
        <div className="@container min-w-0 flex-1 overflow-y-auto">{content}</div>
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
          <div data-card-overlay="" className="@container min-h-full">
            {content}
          </div>
        )}
      </Dialog>
    ),
  }
}
