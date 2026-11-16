import { type PointerEvent, useRef, useState } from "react"
import { useTranslation } from "react-i18next"

import type { CardRef } from "../../../battle/contract"
import { Button } from "../../../components/ui/Button"
import type { BattleMatReach } from "../../../settings"
import { ArtCrop } from "../table/Cards"
import {
  CARD_ART,
  cropRect,
  frameFrom,
  type Framing,
  framingOf,
  keptSize,
  MAX_ZOOM,
  moveFrame,
  type Pan,
  type Size,
} from "./mat-crop"

/** What is being framed: a picture of the player's own, or the art of a card. */
export type CropSource =
  | {
      readonly kind: "file"
      readonly file: Blob
      /** An address of `file` that an image can show; the caller made it and frees it. */
      readonly url: string
    }
  | { readonly kind: "card"; readonly card: CardRef }

/** What the player decided: the frame, and for a picture of their own the copy to keep. */
export interface Framed {
  readonly framing: Framing
  readonly kept: { readonly picture: Blob; readonly size: Size } | null
}

/** A smaller copy of a picture too large to keep; null where the browser cannot draw one. */
function shrink(image: HTMLImageElement, size: Size): Promise<Blob | null> {
  const canvas = document.createElement("canvas")
  canvas.width = size.w
  canvas.height = size.h
  const context = canvas.getContext("2d")
  if (context === null) return Promise.resolve(null)
  context.drawImage(image, 0, 0, size.w, size.h)
  return new Promise((resolve) => {
    canvas.toBlob(resolve, "image/webp", 0.9)
  })
}

const percent = (share: number): string => `${String(share * 100)}%`
const CENTRE: Pan = { x: 0.5, y: 0.5 }

/**
 * Framing a picture for the mat. The whole picture shows, with a frame of the mat's shape on it:
 * the frame is dragged and sized, and what it leaves out turns grey. The picture itself is kept
 * whole, so that the frame can be changed later and a larger mat can show more of it.
 */
export function MatCropper({
  source,
  aspect,
  reach,
  initial,
  onDone,
  onCancel,
}: {
  readonly source: CropSource
  /** Width over height of the mat's picture as it is laid out now. */
  readonly aspect: number
  readonly reach: BattleMatReach
  /** The frame to start from, when the picture was framed before. */
  readonly initial?: Framing | undefined
  readonly onDone: (framed: Framed) => void
  readonly onCancel: () => void
}) {
  const { t } = useTranslation()
  // A card's art has one size; a picture of the player's own tells its size once it has loaded.
  const known = source.kind === "card" ? CARD_ART : null
  const [natural, setNatural] = useState<Size | null>(known)
  const [frame, setFrame] = useState(() =>
    known !== null && initial !== undefined
      ? frameFrom(known, initial, aspect)
      : { zoom: 1, pan: CENTRE },
  )
  const [working, setWorking] = useState(false)
  const image = useRef<HTMLImageElement>(null)
  const stage = useRef<HTMLDivElement>(null)
  const drag = useRef<{ x: number; y: number } | null>(null)

  const rect = natural === null ? null : cropRect(natural, aspect, frame.zoom, frame.pan)
  // The frame as shares of the picture: where its edges lie on the shown copy.
  const edges =
    rect === null || natural === null
      ? null
      : {
          left: rect.x / natural.w,
          top: rect.y / natural.h,
          right: 1 - (rect.x + rect.w) / natural.w,
          bottom: 1 - (rect.y + rect.h) / natural.h,
        }
  const clip =
    edges === null
      ? undefined
      : `inset(${percent(edges.top)} ${percent(edges.right)} ${percent(edges.bottom)} ${percent(edges.left)})`
  const onMove = (event: PointerEvent) => {
    if (drag.current === null || natural === null || rect === null) return
    const shownW = stage.current?.clientWidth ?? 0
    if (shownW <= 0) return
    const dx = event.clientX - drag.current.x
    const dy = event.clientY - drag.current.y
    drag.current = { x: event.clientX, y: event.clientY }
    setFrame((current) => ({
      zoom: current.zoom,
      pan: moveFrame(natural, rect, current.pan, shownW, dx, dy),
    }))
  }
  const finish = async () => {
    if (natural === null || rect === null) return
    setWorking(true)
    const framing = framingOf(natural, rect, aspect, reach)
    if (source.kind === "card") {
      onDone({ framing, kept: null })
      return
    }
    const size = keptSize(natural)
    const smaller =
      size === natural || image.current === null ? null : await shrink(image.current, size)
    onDone({
      framing,
      kept: smaller === null ? { picture: source.file, size: natural } : { picture: smaller, size },
    })
  }
  return (
    <div className="bp-crop">
      <p className="bp-crop-hint">{t("play.settings.upload.dragHint")}</p>
      {/* The picture's box is the stage: the frame is drawn on it and dragged over it. */}
      <div
        ref={stage}
        className={source.kind === "card" ? "bp-crop-stage is-card" : "bp-crop-stage"}
        data-hit-area=""
        onPointerDown={(event) => {
          drag.current = { x: event.clientX, y: event.clientY }
          event.currentTarget.setPointerCapture(event.pointerId)
        }}
        onPointerMove={onMove}
        onPointerUp={() => {
          drag.current = null
        }}
        onPointerCancel={() => {
          drag.current = null
        }}
      >
        {source.kind === "card" ? (
          <>
            <span className="bp-crop-art is-left-out">
              <ArtCrop card={source.card} />
            </span>
            <span className="bp-crop-kept" style={{ clipPath: clip }}>
              <ArtCrop card={source.card} />
            </span>
          </>
        ) : (
          <>
            <img
              ref={image}
              className="bp-crop-whole"
              src={source.url}
              alt=""
              draggable={false}
              onLoad={(event) => {
                const size = {
                  w: event.currentTarget.naturalWidth,
                  h: event.currentTarget.naturalHeight,
                }
                setNatural(size)
                if (initial !== undefined) setFrame(frameFrom(size, initial, aspect))
              }}
            />
            {clip !== undefined ? (
              <img
                className="bp-crop-kept"
                src={source.url}
                alt=""
                draggable={false}
                style={{ clipPath: clip }}
              />
            ) : null}
          </>
        )}
        {edges !== null ? (
          <div
            className="bp-crop-frame"
            style={{
              left: percent(edges.left),
              top: percent(edges.top),
              right: percent(edges.right),
              bottom: percent(edges.bottom),
            }}
          />
        ) : null}
      </div>
      <label className="bp-set-slider">
        <span aria-hidden="true">{t("play.settings.upload.zoom")}</span>
        <input
          type="range"
          aria-label={t("play.settings.upload.zoom")}
          min={1}
          max={MAX_ZOOM}
          step={0.01}
          value={frame.zoom}
          onChange={(event) => {
            const zoom = Number(event.target.value)
            setFrame((current) => ({ zoom, pan: current.pan }))
          }}
        />
        <output>{t("play.settings.upload.zoomValue", { n: Math.round(frame.zoom * 100) })}</output>
      </label>
      <div className="bp-crop-actions">
        <Button variant="ghost" onClick={onCancel}>
          {t("play.settings.upload.cancel")}
        </Button>
        <Button
          variant="primary"
          disabled={natural === null || working}
          onClick={() => {
            void finish()
          }}
        >
          {t("play.settings.upload.use")}
        </Button>
      </div>
    </div>
  )
}
