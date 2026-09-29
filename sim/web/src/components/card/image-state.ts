import type { CardSummary, ImageIndex } from "../../data"

/** True when data saver is holding back a loadable image, so a caller should offer a button. */
export function imageHeldBack(images: ImageIndex | undefined, summary: CardSummary): boolean {
  const asset = images?.asset(summary.printingId, summary.faceId)
  return (
    images?.cardImage(summary.printingId, summary.faceId) !== undefined &&
    asset?.["availability"] === "available" &&
    asset["publication_state"] === "approved"
  )
}
