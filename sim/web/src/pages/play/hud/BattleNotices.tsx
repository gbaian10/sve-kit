import type { CSSProperties } from "react"
import { useTranslation } from "react-i18next"

import type { Notice } from "../../../battle/contract"
import { useCards } from "../battle-cards"

export function BattleNotices({
  notices,
  dismiss,
  style,
}: {
  readonly notices: readonly Notice[]
  readonly dismiss: (id: string) => void
  readonly style?: CSSProperties
}) {
  const { t } = useTranslation()
  const cards = useCards()
  if (notices.length === 0) return null
  return (
    <details className="bp-revelations" style={style}>
      <summary>{t("play.notices.title", { count: notices.length })}</summary>
      <div className="bp-revelations-log">
        {notices.map((notice) => (
          <section key={notice.id}>
            <h2>{t(`play.notices.${notice.kind}`)}</h2>
            {notice.cards.length === 0 ? (
              <p>{t("play.notices.empty")}</p>
            ) : (
              <ul>
                {notice.cards.map(({ card, owner }, index) => {
                  const name = cards.find(card)?.name
                  return (
                    <li key={`${owner}-${String(index)}`}>
                      <bdi lang={name?.lang}>{name?.text ?? card.printing_id}</bdi>
                    </li>
                  )
                })}
              </ul>
            )}
            <button
              type="button"
              onClick={() => {
                dismiss(notice.id)
              }}
            >
              {t("play.notices.dismiss")}
            </button>
          </section>
        ))}
      </div>
    </details>
  )
}
