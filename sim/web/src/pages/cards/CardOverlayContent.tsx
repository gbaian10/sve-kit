import type { CardRouteInput } from "../../data"
import { type CardFrame, CardView } from "../card/CardPage"
import { useCardContent } from "../card/useCardContent"

/** The card inside the list's panel or modal: the page's content, located the same way. */
export function CardOverlayContent({
  input,
  frame,
}: {
  readonly input: CardRouteInput
  readonly frame: CardFrame
}) {
  const content = useCardContent(input)
  return <CardView {...content} frame={frame} />
}
