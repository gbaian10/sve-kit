import { Fragment, useMemo } from "react"

import { usePrefs } from "../../settings"
import { inlinePieces, type SymbolPiece } from "./textSymbols"

/**
 * One official icon in running text. It carries the marker's text as alt, so a copy of the text
 * reads as the official page does; the optional label beside keyword icons follows the
 * symbol-labels setting. Sized to the text so it sits on the line like a glyph at any line height.
 */
export function SymbolIcon({ piece }: { readonly piece: SymbolPiece }) {
  const { symbolLabels } = usePrefs()
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
export function InlineText({ text }: { readonly text: string }) {
  const pieces = useMemo(() => inlinePieces(text), [text])
  return (
    <>
      {pieces.map((piece, index) =>
        piece.kind === "text" ? (
          <Fragment key={index}>{piece.text}</Fragment>
        ) : (
          <SymbolIcon key={index} piece={piece} />
        ),
      )}
    </>
  )
}
