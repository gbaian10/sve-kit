import { Fragment, type ReactNode, useMemo } from "react"

import type { TextRange } from "../../data"
import { usePrefs } from "../../settings"
import { inlinePieces, type SymbolPiece } from "./textSymbols"

// Alt preserves the marker's meaning; glyph-sized icons keep card text in normal flow.
export function SymbolIcon({
  piece,
  labels,
}: {
  readonly piece: SymbolPiece
  /** Overrides the preference, so a settings preview can show both choices side by side. */
  readonly labels?: boolean | undefined
}) {
  const prefs = usePrefs()
  const symbolLabels = labels ?? prefs.symbolLabels
  return (
    <>
      <img
        src={piece.symbol.icon}
        alt={piece.alt}
        draggable={false}
        className="inline-block h-[1.25em] w-auto align-[-0.3em] select-none"
      />
      {symbolLabels && piece.symbol.kind === "keyword" && (
        <span aria-hidden="true" className="mx-0.5 text-[0.85em] font-semibold">
          {piece.label}
        </span>
      )}
    </>
  )
}

/** Card text without annotations, with its official inline icons; everything stays inline. */
export function InlineText({
  text,
  emphasis = false,
  labels,
  changes,
}: {
  readonly text: string
  readonly emphasis?: boolean
  /** Overrides the symbol-label preference (settings previews only). */
  readonly labels?: boolean | undefined
  readonly changes?: { readonly kind: "removed" | "added"; readonly ranges: readonly TextRange[] }
}) {
  const { termEmphasis } = usePrefs()
  const pieces = useMemo(() => inlinePieces(text), [text])
  const mark = (content: ReactNode, key: number) =>
    changes?.kind === "removed" ? (
      <del key={key}>{content}</del>
    ) : (
      <ins key={key} className="decoration-2 underline-offset-2">
        {content}
      </ins>
    )
  const plain = (start: number, end: number): ReactNode => {
    const content: ReactNode[] = []
    let cursor = start
    for (const range of changes?.ranges ?? []) {
      const left = Math.max(start, range.start)
      const right = Math.min(end, range.end)
      if (left >= right) continue
      if (cursor < left) content.push(text.slice(cursor, left))
      content.push(mark(text.slice(left, right), left))
      cursor = right
    }
    if (cursor < end) content.push(text.slice(cursor, end))
    return content
  }
  const content = (start: number, end: number) =>
    pieces.flatMap((piece, index) => {
      if (piece.end <= start || piece.start >= end) return []
      if (piece.kind === "text")
        return [
          <Fragment key={index}>
            {plain(Math.max(start, piece.start), Math.min(end, piece.end))}
          </Fragment>,
        ]
      const icon = <SymbolIcon key={index} piece={piece} labels={labels} />
      return [
        changes?.ranges.some((range) => range.start < piece.end && range.end > piece.start)
          ? mark(icon, index)
          : icon,
      ]
    })
  // Parse emphasis on the full source before applying marks, including icons inside a heading.
  const groups: ReactNode[] = []
  let cursor = 0
  if (emphasis && termEmphasis)
    for (const match of text.matchAll(/【[^】]+】/gu)) {
      const end = match.index + match[0].length
      if (
        pieces.some(
          (piece) =>
            piece.kind === "symbol" &&
            ((piece.start < match.index && match.index < piece.end) ||
              (piece.start < end && end < piece.end)),
        )
      )
        continue
      if (cursor < match.index)
        groups.push(<Fragment key={cursor}>{content(cursor, match.index)}</Fragment>)
      groups.push(<strong key={match.index}>{content(match.index, end)}</strong>)
      cursor = end
    }
  if (cursor < text.length)
    groups.push(<Fragment key={cursor}>{content(cursor, text.length)}</Fragment>)
  return <>{groups}</>
}
