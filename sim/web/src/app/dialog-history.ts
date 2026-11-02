import { useCallback, useEffect, useMemo, useRef } from "react"
import { useLocation, useNavigate } from "react-router"

interface DialogState {
  /** The topmost open dialog. */
  readonly dialog?: string
  /** Every open dialog, outermost first, so a dialog opened inside another keeps it open. */
  readonly dialogs?: readonly string[]
}

function dialogsOf(state: unknown): readonly string[] {
  if (typeof state !== "object" || state === null) return []
  const { dialog, dialogs } = state as DialogState
  if (Array.isArray(dialogs)) return dialogs.filter((value) => typeof value === "string")
  return typeof dialog === "string" ? [dialog] : []
}

type Stage = "idle" | "pushing" | "pushed"

/**
 * Lets the browser back button close an open dialog: opening pushes a history entry (same URL,
 * `state.dialog = id`) through the router, so plain history.pushState never fights the router;
 * closing from the UI pops that entry; popping it from the browser calls `onClose`.
 */
export function useDialogHistory(id: string, open: boolean, onClose: () => void): () => void {
  const navigate = useNavigate()
  const location = useLocation()
  const stage = useRef<Stage>("idle")
  const onCloseRef = useRef(onClose)
  useEffect(() => {
    onCloseRef.current = onClose
  })
  const openIds = useMemo(() => dialogsOf(location.state), [location.state])
  const shown = openIds.includes(id)

  useEffect(() => {
    if (!open) {
      stage.current = "idle"
      return
    }
    if (shown) {
      stage.current = "pushed"
      return
    }
    // Our entry is gone while the dialog is still open: the user pressed back.
    if (stage.current === "pushed") {
      stage.current = "idle"
      onCloseRef.current()
      return
    }
    if (stage.current === "idle") {
      stage.current = "pushing"
      const base: Record<string, unknown> =
        typeof location.state === "object" && location.state !== null
          ? (location.state as Record<string, unknown>)
          : {}
      void navigate(location.pathname + location.search, {
        state: { ...base, dialog: id, dialogs: [...openIds, id] },
      })
    }
  }, [open, shown, openIds, id, navigate, location.pathname, location.search, location.state])

  // Close from the UI: drop the pushed entry too, so history does not keep a dead "dialog open" state.
  return () => {
    const pop = stage.current === "pushed" && shown
    stage.current = "idle"
    onCloseRef.current()
    if (pop) void navigate(-1)
  }
}

/**
 * Pops history entries until none of the open dialogs is left. Each dialog that stays mounted sees
 * its entry go as a back press and closes; one that unmounted (its layout changed) leaves no dead
 * entry behind. It steps back one entry at a time: entries pushed over a dialog without one of
 * their own (a card opened from it keeps the dialog's state) count too.
 */
export function useDropDialogs(): () => void {
  const navigate = useNavigate()
  const location = useLocation()
  const dropping = useRef(false)
  const inDialog = dialogsOf(location.state).length > 0
  // Each step back lands on a new entry; keep stepping while it still belongs to a dialog.
  useEffect(() => {
    if (!dropping.current) return
    if (inDialog) void navigate(-1)
    else dropping.current = false
  }, [inDialog, location.key, navigate])
  return useCallback(() => {
    if (!inDialog) return
    dropping.current = true
    void navigate(-1)
  }, [inDialog, navigate])
}
