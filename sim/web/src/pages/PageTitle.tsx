import type { ReactNode } from "react"

/** Placeholder page heading until each page gets its designed header. */
export function PageTitle({ children }: { readonly children: ReactNode }) {
  return <h1 className="py-4 text-24 font-bold">{children}</h1>
}
