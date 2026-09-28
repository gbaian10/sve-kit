import { X } from "lucide-react"
import { type ReactNode, useCallback, useMemo, useState } from "react"
import { useTranslation } from "react-i18next"

import { Button } from "./Button"
import { ToastContext, type ToastOptions } from "./toast-context"

interface ToastItem extends ToastOptions {
  readonly id: number
}

const DEFAULT_DURATION_MS = 5000

export function ToastProvider({ children }: { readonly children: ReactNode }) {
  const { t } = useTranslation()
  const [items, setItems] = useState<readonly ToastItem[]>([])

  const dismiss = useCallback((id: number) => {
    setItems((current) => current.filter((item) => item.id !== id))
  }, [])

  const show = useCallback(
    (options: ToastOptions) => {
      const id = Date.now() + Math.random()
      setItems((current) => [...current, { ...options, id }])
      window.setTimeout(() => {
        dismiss(id)
      }, options.durationMs ?? DEFAULT_DURATION_MS)
    },
    [dismiss],
  )

  const api = useMemo(() => ({ show }), [show])

  return (
    <ToastContext.Provider value={api}>
      {children}
      {/* Announced politely; visually a stack above the bottom bar (design §7: radius 14, shadow). */}
      <div
        aria-live="polite"
        className="pointer-events-none fixed inset-x-4 bottom-24 z-50 flex flex-col items-center gap-2 lg:bottom-6"
      >
        {items.map((item) => (
          <div
            key={item.id}
            role="status"
            className="pointer-events-auto flex w-full max-w-md items-center gap-2 rounded-block border border-border bg-surface-2 py-2 pr-1 pl-4 text-14 text-text-1 shadow-lg"
          >
            <span className="flex-1">{item.message}</span>
            {item.action && (
              <Button
                variant="ghost"
                size="md"
                className="text-accent-text"
                onClick={() => {
                  item.action?.onClick()
                  dismiss(item.id)
                }}
              >
                {item.action.label}
              </Button>
            )}
            <Button
              variant="ghost"
              size="icon"
              aria-label={t("dialog.close")}
              onClick={() => {
                dismiss(item.id)
              }}
            >
              <X className="size-4" aria-hidden="true" />
            </Button>
          </div>
        ))}
      </div>
    </ToastContext.Provider>
  )
}
