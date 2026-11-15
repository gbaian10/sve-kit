import { ChevronLeft } from "lucide-react"
import { useTranslation } from "react-i18next"

import type { CardRef } from "../../../battle/contract"
import { type KnownCard, useCards } from "../battle-cards"

/** The card's kind in words, for the kinds the snapshot names. */
function CardKind({ known }: { readonly known: KnownCard }) {
  const { t } = useTranslation()
  const code = known.summary.typeCode
  if (code === "leader") return t("play.zone.leader")
  if (code === "follower") return t("play.info.kind.follower")
  if (code === "spell") return t("play.info.kind.spell")
  if (code === "amulet") return t("play.info.kind.amulet")
  return "—"
}

/** The large image on top, then what the snapshot says about the card. */
function CardDetail({ card, where }: { readonly card: CardRef; readonly where: string }) {
  const { t } = useTranslation()
  const cards = useCards()
  const known = cards.find(card)
  if (known === undefined) return null
  const source = cards.images?.cardImage(known.summary.printingId, known.summary.faceId)?.src
  const { cost, attack, defense } = known.summary
  return (
    <div className="bp-detail">
      <div className="bp-detail-img">
        {source !== undefined && (
          <img
            src={source}
            alt={t("card.imageAlt", { name: known.name.text, cardNo: known.summary.cardNo })}
          />
        )}
      </div>
      <h2 lang={known.name.lang}>{known.name.text}</h2>
      {known.original !== undefined ? (
        <div className="bp-detail-ja" lang={known.original.lang}>
          {known.original.text}
        </div>
      ) : null}
      <dl className="bp-detail-facts">
        <dt>{t("play.info.class")}</dt>
        <dd>{known.classLabel ?? "—"}</dd>
        <dt>{t("play.info.kindLabel")}</dt>
        <dd>
          <CardKind known={known} />
        </dd>
      </dl>
      {known.summary.typeCode !== "leader" && (cost !== null || attack !== null) ? (
        <div className="bp-detail-stats">
          {cost !== null ? (
            <span>
              {t("play.stat.cost")} <b className="bp-chip is-cost">{cost}</b>
            </span>
          ) : null}
          {attack !== null && defense !== null ? (
            <>
              <span>
                {t("play.stat.attack")} <b className="bp-chip is-atk">{attack}</b>
              </span>
              <span>
                {t("play.stat.defense")} <b className="bp-chip is-def">{defense}</b>
              </span>
            </>
          ) : null}
        </div>
      ) : null}
      <div className="bp-detail-sub">
        {known.summary.cardNo} · {where}
      </div>
    </div>
  )
}

/** The card-info column. Until cards can be picked it shows the viewer's own leader. */
export function Sidebar({
  card,
  where,
  width,
  overlay,
  onClose,
}: {
  readonly card: CardRef
  readonly where: string
  readonly width: number
  readonly overlay: boolean
  readonly onClose: () => void
}) {
  const { t } = useTranslation()
  return (
    <aside
      className={overlay ? "bp-sidebar is-overlay" : "bp-sidebar"}
      style={{ width }}
      aria-label={t("play.info.title")}
      translate="no"
    >
      <header>
        <span>{t("play.info.title")}</span>
        <button type="button" onClick={onClose} aria-label={t("play.info.collapse")}>
          <ChevronLeft aria-hidden="true" />
        </button>
      </header>
      <CardDetail card={card} where={where} />
    </aside>
  )
}
