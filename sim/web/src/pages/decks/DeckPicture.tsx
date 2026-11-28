import { useEffect, useRef, useState } from "react"
import { useTranslation } from "react-i18next"

import { cn } from "../../components/ui/cn"
import { Notice } from "../../components/ui/Notice"
import { type DeckImageInput, renderDeckImage } from "./deck-image"

// Decoded card images by URL, so switching back to an input already shown redraws without waiting
// for the network or a decode. Small card files are a few kilobytes; the oldest go past the cap.
const CACHE_LIMIT = 400
const cache = new Map<string, Promise<HTMLImageElement | null>>()

function loadPreviewImage(src: string): Promise<HTMLImageElement | null> {
  const hit = cache.get(src)
  if (hit) {
    cache.delete(src)
    cache.set(src, hit)
    return hit
  }
  // Shown on the page only, never exported: no `crossOrigin`, so it shares the cached response
  // the card grid already fetched.
  const image = new Image()
  image.src = src
  const loading = image.decode().then(
    () => image,
    () => {
      // A failed download is tried again next time rather than remembered.
      cache.delete(src)
      return null
    },
  )
  cache.set(src, loading)
  for (const key of cache.keys()) {
    if (cache.size <= CACHE_LIMIT) break
    cache.delete(key)
  }
  return loading
}

/**
 * The deck picture drawn on the page (the export dialog's small picture, without a file). `input`
 * is `null` while card image rows are still arriving; the previous picture is then put away.
 */
export function DeckPicture({
  input,
  label,
}: {
  readonly input: DeckImageInput | null
  readonly label: string
}) {
  const { t } = useTranslation()
  const host = useRef<HTMLDivElement>(null)
  const [drawn, setDrawn] = useState<{
    readonly input: DeckImageInput
    readonly failed: boolean
  } | null>(null)
  useEffect(() => {
    if (input === null) return
    let active = true
    renderDeckImage(input, loadPreviewImage).then(
      (canvas) => {
        if (!active) return
        canvas.className = "block h-auto w-full"
        host.current?.replaceChildren(canvas)
        setDrawn({ input, failed: false })
      },
      () => {
        if (active) setDrawn({ input, failed: true })
      },
    )
    return () => {
      active = false
    }
  }, [input])
  const shown = input !== null && drawn !== null && !drawn.failed
  const failed = input !== null && drawn?.input === input && drawn.failed
  const stale = shown && drawn.input !== input
  return (
    <>
      {/* While another input draws (a few milliseconds once its images are cached) the last
          picture stays, dimmed, rather than blinking away; it is hidden while images are unknown. */}
      <div
        ref={host}
        role="img"
        aria-label={label}
        aria-busy={stale}
        hidden={!shown}
        className={cn(
          "overflow-hidden rounded-button border border-border transition-opacity",
          stale && "opacity-50",
        )}
      />
      {/* Kept in the page so the live region announces when loading starts. */}
      <p role="status" className="text-13 text-text-2 empty:hidden">
        {(!shown || stale) && !failed ? t("decks.importPictureLoading") : ""}
      </p>
      {failed && (
        <Notice tone="danger" role="alert">
          {t("decks.drawImageFailed")}
        </Notice>
      )}
    </>
  )
}
