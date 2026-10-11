import { Fragment, useMemo } from "react"

import { usePrefs } from "../../settings"
import { inlinePieces } from "./textSymbols"

/**
 * Card text with its official inline icons. Each icon carries the marker's text as alt, so a
 * copy of the text reads as the official page does; the optional label beside keyword icons
 * follows the symbol-labels setting. Everything stays inline so any element a browser extension
 * inserts (ruby, mark) joins the same text flow.
 */
export function InlineText({ text }: { readonly text: string }) {
  const { symbolLabels } = usePrefs()
  const pieces = useMemo(() => inlinePieces(text), [text])
  return (
    <>
      {pieces.map((piece, index) =>
        piece.kind === "text" ? (
          <Fragment key={index}>{piece.text}</Fragment>
        ) : (
          <Fragment key={index}>
            <img
              src={piece.symbol.icon}
              alt={piece.alt}
              draggable={false}
              // Sized to the text so the icon sits on the line like a glyph at any line height.
              className="inline-block h-[1.25em] w-auto align-[-0.3em] select-none"
            />
            {symbolLabels && piece.symbol.kind === "keyword" && (
              <span aria-hidden="true" className="mx-0.5 text-[0.85em] font-semibold">
                {piece.label}
              </span>
            )}
          </Fragment>
        ),
      )}
    </>
  )
}
