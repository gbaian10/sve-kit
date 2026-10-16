import type { TFunction } from "i18next"
import { useState } from "react"
import { useTranslation } from "react-i18next"

import type { CardSummary, ImageIndex } from "../../data"
import type { DisplayedText } from "../../domain/nameDisplay"
import { cn } from "../ui/cn"
import { CardPlaceholder } from "./CardPlaceholder"

/**
 * `identify`: the image stands alone, alt names the card; `redundant`: the same text is next to it,
 * so alt is empty; `decorative`: no meaning at all.
 */
type ImageAlt = "identify" | "redundant" | "decorative"
type CandidatePolicy = "responsive" | "largest"

export interface CardImageProps {
  readonly summary: CardSummary
  readonly name: DisplayedText
  /** Undefined until the image file index has loaded; the placeholder shows meanwhile. */
  readonly images: ImageIndex | undefined
  readonly alt: ImageAlt
  /** The `sizes` attribute for this slot, chosen by the layout that places the image. */
  readonly sizes: string
  /** Choose from every available size, or offer only the largest available file. */
  readonly candidatePolicy?: CandidatePolicy
  /** Must set the width (`w-full`, `w-8`); the height follows the 63:88 ratio. */
  readonly className: string
  /** Lists share their translator instead of subscribing once per image. */
  readonly translate?: TFunction
}

// A fixed slot prevents layout shifts while the image downloads or remains unavailable.
const shownSources = new Set<string>()

export function CardImage(props: CardImageProps) {
  return props.translate ? (
    <ImageContent {...props} t={props.translate} />
  ) : (
    <LocalizedImage {...props} />
  )
}

function LocalizedImage(props: CardImageProps) {
  const { t } = useTranslation()
  return <ImageContent {...props} t={t} />
}

function ImageContent({
  summary,
  name,
  images,
  alt,
  sizes,
  candidatePolicy = "responsive",
  className,
  t,
}: CardImageProps & { readonly t: TFunction }) {
  const [loaded, setLoaded] = useState("")
  const [failed, setFailed] = useState("")
  const asset = images?.asset(summary.printingId, summary.faceId)
  const source = images?.cardImage(summary.printingId, summary.faceId)
  const availability = asset?.["availability"]
  const publication = asset?.["publication_state"]
  const loading =
    images === undefined || images.known?.(summary.printingId, summary.faceId) === false
  const tag =
    source !== undefined && failed === source.src
      ? t("card.imageDownloadError")
      : availability === "missing"
        ? t("card.noImage")
        : publication === "pending"
          ? t("card.imagePending")
          : availability === "unfetched"
            ? t("card.imageUnfetched")
            : loading
              ? t("card.imageLoading")
              : (images.failedFace?.(summary.printingId, summary.faceId) ?? images.failed) &&
                  asset === undefined
                ? t("card.imageError")
                : undefined
  const showImage = source !== undefined && tag === undefined
  // A source this session already showed is in the browser cache: a remounted slot (switching
  // views, returning to a list) paints it at once instead of the placeholder until onLoad.
  const isLoaded = source !== undefined && (loaded === source.src || shownSources.has(source.src))
  const stateLabel = tag ?? (showImage && !isLoaded ? t("card.imageDownloading") : undefined)
  // A landscape card keeps its real proportions inside the portrait slot instead of being cropped.
  const landscape = source !== undefined && source.width > source.height
  // In identify mode the slot itself carries the name, so the card is announced with or without
  // a visible image (the placeholder underneath is decorative); the <img> then stays silent.
  const identify = alt === "identify"
  return (
    <>
      <span
        className={cn(
          "relative block aspect-[63/88] overflow-hidden rounded-card border border-border bg-surface-2",
          className,
        )}
        data-image-printing={summary.printingId}
        data-image-face={summary.faceId}
        data-image-state={
          showImage ? (isLoaded ? "shown" : "loading") : loading ? "loading" : "placeholder"
        }
        role={identify ? "img" : alt === "decorative" ? "presentation" : undefined}
        aria-label={
          identify ? t("card.imageAlt", { name: name.text, cardNo: summary.cardNo }) : undefined
        }
      >
        {!(showImage && isLoaded) && (
          <CardPlaceholder
            pulse={loading || showImage}
            {...(stateLabel === undefined ? {} : { tag: stateLabel })}
          />
        )}
        {showImage && (
          <img
            key={source.src}
            src={source.src}
            srcSet={
              candidatePolicy === "largest"
                ? `${source.src} ${String(source.width)}w`
                : source.srcSet
            }
            sizes={sizes}
            width={source.width}
            height={source.height}
            alt=""
            loading={images?.eager?.(summary.printingId, summary.faceId) ? "eager" : "lazy"}
            decoding="async"
            onError={() => {
              setFailed(source.src)
            }}
            onLoad={() => {
              shownSources.add(source.src)
              setLoaded(source.src)
            }}
            className={cn(
              "absolute inset-0 size-full transition-opacity duration-200",
              landscape ? "object-contain" : "object-cover",
              isLoaded ? "opacity-100" : "opacity-0",
            )}
          />
        )}
      </span>
      {alt !== "decorative" && stateLabel !== undefined && (
        <span className="sr-only">{stateLabel}</span>
      )}
    </>
  )
}
