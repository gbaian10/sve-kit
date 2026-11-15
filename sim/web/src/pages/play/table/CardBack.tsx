import { useTranslation } from "react-i18next"

import { cardBackUrl, useSnapshotRoot } from "../../../data"
import { markCardBackFailed, useCardBackFailed } from "./card-backs"

/** The drawn back: a compass star inside two rings on a night-blue field, gold lines. */
function Star() {
  return (
    <>
      <rect className="bp-back-field" width="100" height="140" rx="6" />
      <rect className="bp-back-rim" x="4" y="4" width="92" height="132" rx="4" />
      <rect className="bp-back-rim is-thin" x="7.5" y="7.5" width="85" height="125" rx="2.5" />
      <g className="bp-back-line">
        <circle cx="50" cy="70" r="31" />
        <circle cx="50" cy="70" r="25" strokeDasharray="1.5 3.5" />
        <path d="M50 14V35M50 105V126M13 70H19M81 70H87" />
        <path d="M13 13 20 20M87 13 80 20M13 127 20 120M87 127 80 120" />
      </g>
      <path
        className="bp-back-mark is-soft"
        d="M50 49 54 66 71 70 54 74 50 91 46 74 29 70 46 66Z"
        transform="rotate(45 50 70)"
      />
      <path className="bp-back-mark" d="M50 40 56 64 80 70 56 76 50 100 44 76 20 70 44 64Z" />
      <circle className="bp-back-core" cx="50" cy="70" r="4" />
    </>
  )
}

/**
 * A card seen from behind: the official back, published with the card images. Where that file is
 * missing the app draws a back of its own, which follows no game's back.
 */
export function CardBack() {
  const { t } = useTranslation()
  const url = cardBackUrl(useSnapshotRoot())
  const failed = useCardBackFailed(url)
  if (failed)
    return (
      <svg
        className="bp-back-img"
        viewBox="0 0 100 140"
        preserveAspectRatio="none"
        role="img"
        aria-label={t("play.card.back")}
      >
        <Star />
      </svg>
    )
  return (
    <img
      className="bp-back-img"
      src={url}
      alt={t("play.card.back")}
      draggable={false}
      onError={() => {
        markCardBackFailed(url)
      }}
    />
  )
}
