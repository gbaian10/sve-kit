import { useEffect, useRef } from "react"
import { useLocation, useNavigate } from "react-router"

interface DialogState {
  readonly dialog?: string
}

function dialogOf(state: unknown): string | undefined {
  if (typeof state !== "object" || state === null) return undefined
  const value = (state as DialogState).dialog
  return typeof value === "string" ? value : undefined
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
  const current = dialogOf(location.state)

  useEffect(() => {
    if (!open) {
      stage.current = "idle"
      return
    }
    if (current === id) {
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
      void navigate(location.pathname + location.search, { state: { ...base, dialog: id } })
    }
  }, [open, current, id, navigate, location.pathname, location.search, location.state])

  // Close from the UI: drop the pushed entry too, so history does not keep a dead "dialog open" state.
  return () => {
    const pop = stage.current === "pushed" && current === id
    stage.current = "idle"
    onCloseRef.current()
    if (pop) void navigate(-1)
  }
}
