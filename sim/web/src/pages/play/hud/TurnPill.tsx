import { useTranslation } from "react-i18next"

import type { TableModel } from "../../../battle/view/table"

/** Whose turn and which phase, with the button that ends the turn. */
export function TurnPill({ model }: { readonly model: TableModel }) {
  const { t } = useTranslation()
  const active = model.self.active ? model.self : model.opponent.active ? model.opponent : null
  const side = model.self.active ? "self" : "opponent"
  return (
    <section className="bp-turn" aria-label={t("play.turn.label")}>
      <span className="bp-turn-no">
        {/* The opening has an active seat once the first player is chosen, and no turn yet. */}
        {active === null || model.phase === null
          ? t("play.turn.opening")
          : t("play.turn.number", { n: active.turns })}
      </span>
      {active !== null && model.phase !== null && (
        <span className={model.self.active ? "bp-turn-phase is-self" : "bp-turn-phase is-opp"}>
          {t(`play.turn.${side}.${model.phase}`)}
        </span>
      )}
      {/* The table cannot be answered yet, so there is no turn of the viewer's to end. */}
      <button type="button" className="bp-end" disabled>
        {t("play.turn.end")}
      </button>
    </section>
  )
}
