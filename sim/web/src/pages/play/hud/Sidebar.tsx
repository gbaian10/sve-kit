import { ChevronLeft, Maximize2, Pin, PinOff } from "lucide-react"
import { useTranslation } from "react-i18next"

import type { CardRef } from "../../../battle/contract"
import type { Inspection } from "../../../battle/view/inspect"
import { CardText } from "../../../components/card/CardText"
import { type KnownCard, useCards } from "../battle-cards"
import { isNamedKeyword } from "../dice/keywords"
import { PIN_KEY } from "../inspector"
import { useCounterText } from "../table/counters"

/** The card's kind in words, for the kinds the snapshot names. */
export function CardKind({ known }: { readonly known: KnownCard }) {
  const { t } = useTranslation()
  const code = known.summary.typeCode
  if (code === "leader") return t("play.zone.leader")
  if (code === "follower") return t("play.info.kind.follower")
  if (code === "spell") return t("play.info.kind.spell")
  if (code === "amulet") return t("play.info.kind.amulet")
  return "—"
}

/** The wording in force for a card, as the card page's own text block. */
export function CardWordingText({ known }: { readonly known: KnownCard }) {
  const { t } = useTranslation()
  const { text } = useCards()
  if (text === undefined || known.wording === undefined)
    return <div className="bp-detail-text is-empty">{t("play.info.noText")}</div>
  return (
    <div className="bp-detail-wording">
      <CardText
        client={text.client}
        edition={text.edition}
        owner={{ kind: "face_revision", id: known.wording }}
        field="effect"
      />
    </div>
  )
}

/** What is being looked at: a face, where it lies and, for a card on the table, its current state. */
export interface Shown {
  readonly card: CardRef
  readonly where: string
  /** Null for a leader, which is not an object of the game. */
  readonly state: Inspection | null
  /** Shown because the pointer rests on it, not because it was selected. */
  readonly preview: boolean
}

/** The large image on top, then the card's current numbers beside what it prints, then its text. */
function CardDetail({ shown, onZoom }: { readonly shown: Shown; readonly onZoom: () => void }) {
  const { t } = useTranslation()
  const counterText = useCounterText()
  const cards = useCards()
  const known = cards.find(shown.card)
  if (known === undefined) return null
  const source = cards.images?.cardImage(known.summary.printingId, known.summary.faceId)?.src
  const { cost, attack, defense } = known.summary
  const state = shown.state
  const stats = state?.card.stats ?? null
  const changed =
    stats !== null && attack !== null && (stats.attack !== attack || stats.life !== defense)
  const under = state?.under == null ? undefined : cards.find(state.under)
  return (
    <div className="bp-detail">
      <button
        type="button"
        className="bp-detail-img"
        onClick={onZoom}
        aria-label={t("play.info.zoom")}
        title={t("play.info.zoom")}
      >
        {source !== undefined && (
          <img
            src={source}
            alt={t("card.imageAlt", { name: known.name.text, cardNo: known.summary.cardNo })}
          />
        )}
        <Maximize2 aria-hidden="true" />
      </button>
      {shown.preview ? <div className="bp-detail-preview">{t("play.info.preview")}</div> : null}
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
          {state?.card.evolution === "super_evolved"
            ? t("play.info.state.superEvolved")
            : state?.card.evolution === "evolved"
              ? t("play.info.state.evolved")
              : null}
          {state?.card.acted === true ? t("play.info.state.acted") : null}
        </dd>
        {state !== null && state.card.keywords.length > 0 ? (
          <>
            <dt>{t("play.info.keywords")}</dt>
            <dd className="bp-detail-list">
              {state.card.keywords.map((keyword) => (
                <span key={keyword}>
                  {isNamedKeyword(keyword) ? t(`play.keyword.${keyword}`) : keyword}
                </span>
              ))}
            </dd>
          </>
        ) : null}
        {state !== null && state.card.counters.length > 0 ? (
          <>
            <dt>{t("play.counter.label")}</dt>
            {/* One entry per kind, set apart by spacing: no language's list punctuation is needed. */}
            <dd className="bp-detail-list">
              {state.card.counters.map((counter) => (
                <span key={counter.name}>{counterText(counter)}</span>
              ))}
            </dd>
          </>
        ) : null}
        {state !== null && state.traits.length > 0 ? (
          <>
            <dt>{t("play.info.traits")}</dt>
            {/* Traits arrive as the rules terms of the card text. */}
            <dd className="bp-detail-list" lang="ja">
              {state.traits.map((trait) => (
                <span key={trait}>{trait}</span>
              ))}
            </dd>
          </>
        ) : null}
      </dl>
      {known.summary.typeCode !== "leader" && (cost !== null || attack !== null) ? (
        <div className="bp-detail-stats">
          {cost !== null ? (
            <span>
              {t("play.stat.cost")} <b className="bp-chip is-cost">{cost}</b>
              {state !== null && state.card.cost !== null && state.card.cost !== cost ? (
                <small>{t("play.info.costNow", { n: state.card.cost })}</small>
              ) : null}
            </span>
          ) : null}
          {attack !== null && defense !== null ? (
            <>
              <span>
                {t("play.stat.attack")} <b className="bp-chip is-atk">{stats?.attack ?? attack}</b>
                {changed ? <small>{t("play.info.printed", { n: attack })}</small> : null}
              </span>
              <span>
                {t("play.stat.defense")} <b className="bp-chip is-def">{stats?.life ?? defense}</b>
                {changed ? <small>{t("play.info.printed", { n: defense })}</small> : null}
              </span>
            </>
          ) : null}
        </div>
      ) : null}
      {known.summary.typeCode !== "leader" ? <CardWordingText known={known} /> : null}
      {under !== undefined ? (
        <div className="bp-detail-sub">
          {t("play.info.under", { name: under.name.text, no: under.summary.cardNo })}
        </div>
      ) : null}
      <div className="bp-detail-sub">
        {known.summary.cardNo} · {shown.where}
      </div>
    </div>
  )
}

/** The card-info column: the selected card, or the one the pointer rests on. */
export function Sidebar({
  shown,
  width,
  overlay,
  pinned,
  onPin,
  onZoom,
  onClose,
}: {
  readonly shown: Shown
  readonly width: number
  readonly overlay: boolean
  /** Pinned, the column keeps the selected card while the pointer rests on others. */
  readonly pinned: boolean
  readonly onPin: () => void
  readonly onZoom: () => void
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
        <button
          type="button"
          className={pinned ? "bp-pin is-on" : "bp-pin"}
          onClick={onPin}
          aria-pressed={pinned}
          aria-label={pinned ? t("play.info.unpin") : t("play.info.pin")}
          title={pinned ? t("play.info.pinnedHint") : t("play.info.pinHint")}
        >
          {pinned ? <Pin aria-hidden="true" /> : <PinOff aria-hidden="true" />}
          <kbd aria-hidden="true">{PIN_KEY}</kbd>
        </button>
        <button type="button" onClick={onClose} aria-label={t("play.info.collapse")}>
          <ChevronLeft aria-hidden="true" />
        </button>
      </header>
      {/* Keyed by card, so nothing a browser add-on wrote into one card's text leaks into the next. */}
      <CardDetail
        key={`${shown.card.printing_id}:${String(shown.card.face)}:${shown.state?.card.handle ?? ""}`}
        shown={shown}
        onZoom={onZoom}
      />
    </aside>
  )
}
