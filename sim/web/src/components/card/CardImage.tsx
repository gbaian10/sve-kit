import { useState } from "react"
import { useTranslation } from "react-i18next"

import type { CardSummary, ImageIndex } from "../../data"
import type { DisplayedText } from "../../domain/nameDisplay"
import { usePrefs } from "../../settings"
import { cn } from "../ui/cn"
import { TextCard } from "./TextCard"

/**
 * `identify`: the image stands alone, alt names the card; `redundant`: the same text is next to it,
 * so alt is empty; `decorative`: no meaning at all.
 */
type ImageAlt = "identify" | "redundant" | "decorative"

export interface CardImageProps {
  readonly summary: CardSummary
  readonly name: DisplayedText
  /** Undefined until the image file index has loaded; the text card shows meanwhile. */
  readonly images: ImageIndex | undefined
  readonly alt: ImageAlt
  /** The `sizes` attribute for this slot, chosen by the layout that places the image. */
  readonly sizes: string
  /** `contain` for landscape cards shown in a portrait slot. */
  readonly fit?: "cover" | "contain"
  /** Must set the width (`w-full`, `w-8`); the height follows the 63:88 ratio. */
  readonly className: string
  /**
   * Data saver: images load only when the caller says so (its own button outside any link).
   * Ignored when data saver is off.
   */
  readonly imageWanted?: boolean
}

function hostOf(url: unknown): string {
  if (typeof url !== "string") return ""
  try {
    return new URL(url).hostname
  } catch {
    return ""
  }
}

// One fixed 63:88 slot: the text card sits underneath and the image fades in over it when it has
// loaded, so nothing shifts. Missing, pending and withdrawn images keep the text card and say why.
export function CardImage({
  summary,
  name,
  images,
  alt,
  sizes,
  fit = "cover",
  className,
  imageWanted = false,
}: CardImageProps) {
  const { t } = useTranslation()
  const { dataSaver } = usePrefs()
  // Load state belongs to one image URL: another printing in the same slot starts afresh.
  const [image, setImage] = useState<{
    readonly src: string | undefined
    readonly loaded: boolean
    readonly failed: boolean
  }>({ src: undefined, loaded: false, failed: false })
  const asset = images?.asset(summary.printingId, summary.faceId)
  const source = images?.cardImage(summary.printingId, summary.faceId)
  const { loaded, failed } =
    source !== undefined && image.src === source.src ? image : { loaded: false, failed: false }
  const availability = asset?.["availability"]
  const publication = asset?.["publication_state"]
  const tag =
    publication === "withdrawn"
      ? t("card.withdrawn", {
          reason:
            typeof asset?.["withdrawal_reason"] === "string" ? asset["withdrawal_reason"] : "",
          host: hostOf(asset?.["source_url"]),
        })
      : availability === "missing"
        ? t("card.noImage")
        : publication === "pending" || availability === "unfetched"
          ? t("card.imagePending")
          : undefined
  const showImage =
    source !== undefined && tag === undefined && !failed && (!dataSaver || imageWanted)
  // In identify mode the slot itself carries the name, so the card is announced with or without
  // a visible image (the text card underneath is decorative); the <img> then stays silent.
  const identify = alt === "identify"
  return (
    <span
      className={cn(
        "relative block aspect-[63/88] overflow-hidden rounded-card border border-border bg-surface-2",
        className,
      )}
      role={identify ? "img" : alt === "decorative" ? "presentation" : undefined}
      aria-label={
        identify ? t("card.imageAlt", { name: name.text, cardNo: summary.cardNo }) : undefined
      }
    >
      {!(showImage && loaded) && (
        <TextCard
          name={name}
          classCode={summary.classCode}
          cost={summary.cost}
          attack={summary.attack}
          defense={summary.defense}
          cardNo={summary.cardNo}
          {...(tag === undefined ? (failed ? { tag: t("card.noImage") } : {}) : { tag })}
        />
      )}
      {showImage && (
        <img
          src={source.src}
          srcSet={source.srcSet}
          sizes={sizes}
          width={source.width}
          height={source.height}
          alt=""
          loading="lazy"
          decoding="async"
          onLoad={() => {
            setImage({ src: source.src, loaded: true, failed: false })
          }}
          onError={() => {
            setImage({ src: source.src, loaded: false, failed: true })
          }}
          className={cn(
            "absolute inset-0 size-full transition-opacity duration-200",
            fit === "cover" ? "object-cover" : "object-contain",
            loaded ? "opacity-100" : "opacity-0",
          )}
        />
      )}
    </span>
  )
}
