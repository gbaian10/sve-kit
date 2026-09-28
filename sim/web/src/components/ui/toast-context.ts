import { createContext, useContext } from "react"

export interface ToastOptions {
  readonly message: string
  /** Optional undo-style action; the label must already be translated. */
  readonly action?: { readonly label: string; readonly onClick: () => void }
  readonly durationMs?: number
}

export interface ToastApi {
  readonly show: (options: ToastOptions) => void
}

export const ToastContext = createContext<ToastApi | null>(null)

export function useToast(): ToastApi {
  const api = useContext(ToastContext)
  if (!api) throw new Error("useToast needs a ToastProvider above it")
  return api
}
