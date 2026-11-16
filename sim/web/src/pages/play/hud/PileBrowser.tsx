import { X } from "lucide-react"
import { useState } from "react"
import { useTranslation } from "react-i18next"

import type { SeatTable, TableCard } from "../../../battle/view/table"
import { type KnownCard, useCards } from "../battle-cards"
import type { BrowsablePile, Zoomed } from "../inspector"
import { CardFace } from "../table/Cards"
import { Overlay } from "./Overlay"

interface Section {
  readonly label?: string
  readonly cards: readonly TableCard[]
  /** Cards only this seat may see. */
  readonly secret?: boolean
}

/** Counts of one fact over the cards, most frequent first. */
function tally(values: readonly (string | null)[]): [string, number][] {
  const counts = new Map<string, number>()
  for (const value of values) if (value !== null) counts.set(value, (counts.get(value) ?? 0) + 1)
  return [...counts].sort((a, b) => b[1] - a[1])
}

function Chips({
  label,
  options,
  value,
  onChange,
}: {
  readonly label: string
  readonly options: readonly [string, number][]
  readonly value: string | null
  readonly onChange: (next: string | null) => void
}) {
  if (options.length === 0) return null
  return (
    <div className="bp-stat-group">
      <span>{label}</span>
      {options.map(([name, count]) => (
        <button
          key={name}
          type="button"
          className={value === name ? "bp-stat-chip is-on" : "bp-stat-chip"}
          aria-pressed={value === name}
          onClick={() => {
            onChange(value === name ? null : name)
          }}
        >
          {name}
          <b>{count}</b>
        </button>
      ))}
    </div>
  )
}

/**
 * The cards of one open pile, newest first. A cemetery or a banish zone can be narrowed by card
 * type and class; an evolve deck shows its face-down cards to their owner only.
 */
export function PileBrowser({
  seat,
  pile,
  own,
  onZoom,
  onClose,
}: {
  readonly seat: SeatTable
  readonly pile: BrowsablePile
  readonly own: boolean
  readonly onZoom: (zoomed: Zoomed) => void
  readonly onClose: () => void
}) {
  const { t } = useTranslation()
  const cards = useCards()
  const [kind, setKind] = useState<string | null>(null)
  const [cls, setCls] = useState<string | null>(null)
  const side = own ? "self" : "opponent"
  const kindOf = (known: KnownCard | undefined): string | null => {
    const code = known?.summary.typeCode
    if (code === "follower" || code === "spell" || code === "amulet")
      return t(`play.info.kind.${code}`)
    return null
  }

  let title: string
  let sections: Section[]
  if (pile === "evolveDeck") {
    const up = seat.evolveDeck.cards.filter((card) => !card.faceDown)
    const down = seat.evolveDeck.cards.filter((card) => card.faceDown)
    const hidden = seat.evolveDeck.count - up.length
    title = t(`play.pile.evolveTitle.${side}`, { down: hidden, up: up.length })
    sections = [
      own
        ? { label: t("play.pile.faceDownOwn", { n: hidden }), cards: down, secret: true }
        : { label: t("play.pile.faceDownHidden", { n: hidden }), cards: [] },
      { label: t("play.pile.faceUp", { n: up.length }), cards: up },
    ]
  } else {
    const zone = pile === "cemetery" ? seat.cemetery : seat.banish
    title = t(`play.pile.title.${side}`, { zone: t(`play.zone.${pile}`), n: zone.count })
    sections = [{ cards: zone.cards }]
  }
  const stats = pile !== "evolveDeck"
  const all = sections.flatMap((section) => section.cards)
  const facts = new Map(
    all.map((card) => {
      const known = cards.find(card.face)
      return [card.handle, { kind: kindOf(known), cls: known?.classLabel ?? null }] as const
    }),
  )
  const keep = (card: TableCard) => {
    const fact = facts.get(card.handle)
    return (kind === null || fact?.kind === kind) && (cls === null || fact?.cls === cls)
  }
  return (
    <Overlay label={title} onClose={onClose}>
      <div className="bp-browser" translate="no">
        <header>
          <h2>{title}</h2>
          <button type="button" onClick={onClose} aria-label={t("play.info.close")}>
            <X aria-hidden="true" />
          </button>
        </header>
        {stats && all.length > 0 ? (
          <div className="bp-stats">
            <div className="bp-stat-group">
              <span>{t("play.pile.total")}</span>
              <button
                type="button"
                className={kind === null && cls === null ? "bp-stat-chip is-on" : "bp-stat-chip"}
                aria-pressed={kind === null && cls === null}
                onClick={() => {
                  setKind(null)
                  setCls(null)
                }}
              >
                {t("play.pile.all")}
                <b>{all.length}</b>
              </button>
            </div>
            <Chips
              label={t("play.pile.kind")}
              options={tally([...facts.values()].map((fact) => fact.kind))}
              value={kind}
              onChange={setKind}
            />
            <Chips
              label={t("play.pile.class")}
              options={tally([...facts.values()].map((fact) => fact.cls))}
              value={cls}
              onChange={setCls}
            />
          </div>
        ) : null}
        {sections.map((section, index) => {
          const shown = section.cards.toReversed().filter(keep)
          return (
            <section key={index} className="bp-browser-sec">
              {section.label !== undefined ? <h3>{section.label}</h3> : null}
              {shown.length === 0 && (section.cards.length > 0 || section.label === undefined) ? (
                <p className="bp-empty">
                  {section.cards.length > 0 ? t("play.pile.noMatch") : t("play.pile.none")}
                </p>
              ) : null}
              <div className="bp-browser-grid">
                {shown.map((card) => (
                  <button
                    key={card.handle}
                    type="button"
                    className={section.secret === true ? "is-secret" : undefined}
                    onClick={() => {
                      onZoom({ face: card.face, under: null })
                    }}
                  >
                    <CardFace card={card.face} tag />
                  </button>
                ))}
              </div>
            </section>
          )
        })}
      </div>
    </Overlay>
  )
}
