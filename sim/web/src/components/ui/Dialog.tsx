import { X } from "lucide-react"
import {
  type MouseEvent,
  type ReactNode,
  type SyntheticEvent,
  useEffect,
  useId,
  useRef,
} from "react"
import { useTranslation } from "react-i18next"

import { cn } from "./cn"

export interface DialogProps {
  readonly open: boolean
  readonly onClose: () => void
  readonly title: string
  /**
   * `sheet` rises from the bottom on phones and centres on wide screens; `center` always centres;
   * `card` fills a phone and becomes a wide centred window from the `md` breakpoint.
   */
  readonly variant?: "sheet" | "center" | "card"
  /** No built-in title bar: the body brings its own heading and close control; `title` names the dialog. */
  readonly bare?: boolean
  readonly children: ReactNode
  /** Fixed bar under the scrolling body (design: bottom panels keep their buttons pinned). */
  readonly footer?: ReactNode
  readonly className?: string
}

// Native <dialog> does the modal work (top layer, inert background, focus trap). What it does
// not do reliably across browsers is picked up here: initial focus on the close button, returning
// focus to the opener, Escape and backdrop clicks going through onClose so the caller stays in charge.
export function Dialog({
  open,
  onClose,
  title,
  variant = "sheet",
  bare = false,
  children,
  footer,
  className,
}: DialogProps) {
  const { t } = useTranslation()
  const ref = useRef<HTMLDialogElement>(null)
  const closeRef = useRef<HTMLButtonElement>(null)
  const bodyRef = useRef<HTMLDivElement>(null)
  const openerRef = useRef<HTMLElement | null>(null)
  const titleId = useId()

  useEffect(() => {
    const dialog = ref.current
    if (!dialog) return
    if (open && !dialog.open) {
      openerRef.current =
        document.activeElement instanceof HTMLElement ? document.activeElement : null
      dialog.showModal()
      ;(closeRef.current ?? bodyRef.current)?.focus()
    } else if (!open && dialog.open) {
      dialog.close()
      openerRef.current?.focus()
      openerRef.current = null
    }
  }, [open])
  // The top layer traps focus but not the wheel: the page behind a modal must not scroll.
  useEffect(() => {
    if (!open) return
    const previous = document.documentElement.style.overflow
    document.documentElement.style.overflow = "hidden"
    return () => {
      document.documentElement.style.overflow = previous
    }
  }, [open])

  const onCancel = (event: SyntheticEvent<HTMLDialogElement>) => {
    event.preventDefault()
    onClose()
  }
  const onBackdropClick = (event: MouseEvent<HTMLDialogElement>) => {
    if (event.target === ref.current) onClose()
  }

  return (
    // eslint-disable-next-line jsx-a11y/click-events-have-key-events, jsx-a11y/no-noninteractive-element-interactions -- a click on the backdrop is a pointer-only shortcut; keyboard users have Escape and the close button
    <dialog
      ref={ref}
      aria-labelledby={bare ? undefined : titleId}
      aria-label={bare ? title : undefined}
      onCancel={onCancel}
      onClick={onBackdropClick}
      className={cn(
        "m-0 max-h-dvh w-full bg-transparent p-0 text-text-1 backdrop:bg-scrim",
        variant === "sheet" &&
          "top-auto bottom-0 max-w-none md:top-1/2 md:bottom-auto md:left-1/2 md:max-w-lg md:-translate-x-1/2 md:-translate-y-1/2",
        variant === "center" && "top-1/2 left-1/2 max-w-lg -translate-x-1/2 -translate-y-1/2",
        variant === "card" &&
          "inset-0 h-dvh max-w-none md:inset-auto md:top-1/2 md:left-1/2 md:h-auto md:max-w-[min(100%-2rem,58rem)] md:-translate-x-1/2 md:-translate-y-1/2",
      )}
    >
      <div
        ref={bodyRef}
        tabIndex={-1}
        className={cn(
          "flex flex-col bg-surface-1 outline-none",
          variant === "sheet" && "max-h-[92dvh] rounded-t-sheet md:rounded-dialog",
          variant === "center" && "max-h-[92dvh] rounded-dialog",
          variant === "card" && "h-dvh max-h-dvh md:h-auto md:max-h-[92dvh] md:rounded-dialog",
          className,
        )}
      >
        {variant === "sheet" && (
          <div
            aria-hidden="true"
            className="mx-auto mt-2 h-1 w-9 rounded-pill bg-border-strong md:hidden"
          />
        )}
        {!bare && (
          <header className="flex items-center justify-between px-4 pt-3 pb-2">
            <h2 id={titleId} className="text-16 font-semibold">
              {title}
            </h2>
            <button
              ref={closeRef}
              type="button"
              onClick={onClose}
              aria-label={t("dialog.close")}
              className="flex size-11 items-center justify-center rounded-button text-text-2 hover:bg-surface-2"
            >
              <X className="size-5" aria-hidden="true" />
            </button>
          </header>
        )}
        <div className={cn("min-h-0 flex-1 overflow-y-auto", bare ? "p-0" : "px-4 pb-4")}>
          {children}
        </div>
        {footer !== undefined && (
          <div className="border-t border-border bg-side px-4 py-3 pb-[max(env(safe-area-inset-bottom),0.75rem)]">
            {footer}
          </div>
        )}
      </div>
    </dialog>
  )
}
