import { useState } from "react"

import type { CardSummary, ImageIndex, ImageSource } from "../../data"
import type { DisplayedText } from "../../domain/nameDisplay"
import { CardImage } from "../card/CardImage"
import type { ResultLabels } from "./labels"

// Missing or failed illustration crops fall back to the full-card thumbnail.
export function ArtThumb({
  summary,
  name,
  images,
  art,
  labels,
}: {
  readonly summary: CardSummary
  readonly name: DisplayedText
  readonly images: ImageIndex | undefined
  readonly art: ImageSource | undefined
  readonly labels: ResultLabels
}) {
  const [failed, setFailed] = useState("")
  if (art === undefined || failed === art.src)
    return (
      <CardImage
        summary={summary}
        name={name}
        images={images}
        alt="redundant"
        size="large"
        className="w-12 shrink-0 self-start"
        translate={labels.t}
      />
    )
  return (
    <span
      data-art-thumb
      data-image-printing={summary.printingId}
      data-image-face={summary.faceId}
      className="relative block aspect-[4/3] w-20 shrink-0 self-start overflow-hidden rounded-sm border border-border bg-surface-2"
    >
      <img
        src={art.src}
        width={art.width}
        height={art.height}
        alt=""
        loading="lazy"
        decoding="async"
        onError={() => {
          setFailed(art.src)
        }}
        className="absolute inset-0 size-full object-cover"
      />
    </span>
  )
}
