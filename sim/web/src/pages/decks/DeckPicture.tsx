import { Fragment, useEffect, useRef, useState } from "react"
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

interface Target {
  readonly key: string
  readonly input: DeckImageInput
}

/**
 * The deck picture drawn on the page (the export dialog's small picture, without a file). `input`
 * is `null` while card image rows may still arrive. A picture is shown only while it matches the
 * current input; an older one keeps its space, unseen, until the new one is drawn.
 */
export function DeckPicture({
  input,
  label,
  cardLang,
}: {
  readonly input: DeckImageInput | null
  readonly label: string
  /** The language card names are in (the deck's edition). */
  readonly cardLang: "ja" | "en"
}) {
  const { t } = useTranslation()
  const host = useRef<HTMLDivElement>(null)
  // Inputs are rebuilt on unrelated renders; the picture follows what it shows, not the object.
  const key = input === null ? null : JSON.stringify(input)
  const [target, setTarget] = useState<Target | null>(null)
  // Waiting (`null`) lets go of the target, so coming back draws again: a failed drawing or a card
  // image that failed to decode gets another try.
  if (key !== (target?.key ?? null))
    setTarget(input === null || key === null ? null : { key, input })
  // Tied to the drawing that made it: a retry of the same deck shows loading, not the old error.
  const [drawn, setDrawn] = useState<{
    readonly target: Target
    readonly failed: boolean
  } | null>(null)
  useEffect(() => {
    if (target === null) return
    let active = true
    renderDeckImage(target.input, loadPreviewImage).then(
      (canvas) => {
        if (!active) return
        canvas.className = "block h-auto w-full"
        host.current?.replaceChildren(canvas)
        setDrawn({ target, failed: false })
      },
      () => {
        if (active) setDrawn({ target, failed: true })
      },
    )
    return () => {
      active = false
    }
  }, [target])
  // The canvas shows a deck's content whichever drawing made it; an error belongs to its drawing.
  const current = key !== null && drawn?.target.key === key && !drawn.failed
  const failed = target !== null && drawn?.target === target && drawn.failed
  return (
    <>
      <div
        ref={host}
        role="img"
        aria-label={label}
        hidden={drawn === null || failed}
        className="overflow-hidden rounded-button border border-border"
        // Hidden in place: the next picture takes over the same space without a jump.
        style={current ? undefined : { visibility: "hidden" }}
      />
      {/* Present before it has text, so the change is announced; unseen while empty. */}
      <p role="status" className={cn("text-13 text-text-2", (current || failed) && "sr-only")}>
        {current || failed ? "" : t("decks.importPictureLoading")}
      </p>
      {failed && (
        <Notice tone="danger" role="alert">
          {t("decks.drawImageFailed")}
        </Notice>
      )}
      {/* The cards the picture shows, for screen readers to check them one by one. */}
      {input !== null && (
        <div className="sr-only">
          {input.sections
            .filter((section) => section.cards.length > 0)
            .map((section) => (
              <Fragment key={section.label}>
                <p>{section.label}</p>
                <ul>
                  {section.cards.map((card, i) => (
                    <li key={`${String(i)}-${card.label}`}>
                      <span lang={cardLang}>{card.label}</span>{" "}
                      {t("decks.importCount", { count: card.count })}
                    </li>
                  ))}
                </ul>
              </Fragment>
            ))}
        </div>
      )}
    </>
  )
}
