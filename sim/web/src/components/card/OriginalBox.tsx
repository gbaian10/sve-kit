import type { ReactNode } from "react"
import { useTranslation } from "react-i18next"

/** The framed original-language text under the translation it belongs to (card text, Q&A, errata). */
export function OriginalBox({ children }: { readonly children: ReactNode }) {
  const { t } = useTranslation()
  return (
    <div
      className="rounded-control border border-border px-3 py-2 text-14 leading-effect text-text-2"
      data-original
    >
      <p className="mb-1 text-12 text-text-3">{t("card.originalText")}</p>
      {children}
    </div>
  )
}

/**
 * A translation followed by its original in a box, line by line when `lines` > 1: line i of the
 * translation is line i of the original (the build derives the translation per source line).
 */
export function PairedLines({
  lines,
  translated,
  original,
}: {
  readonly lines: number
  readonly translated: (line?: number) => ReactNode
  readonly original: ((line?: number) => ReactNode) | null
}) {
  if (lines <= 1)
    return (
      <>
        {translated()}
        {original && <OriginalBox>{original()}</OriginalBox>}
      </>
    )
  return Array.from({ length: lines }, (_, line) => (
    <div key={line} className="space-y-2" data-line={line}>
      {translated(line)}
      {original && <OriginalBox>{original(line)}</OriginalBox>}
    </div>
  ))
}
