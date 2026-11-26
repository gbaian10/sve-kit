import { useState } from "react"
import { useTranslation } from "react-i18next"

import type { Command, Question } from "../../battle/contract"
import type { BattleSource } from "../../battle/source"
import { act, actions, actQuick, quick } from "../../battle/view/actions"
import {
  choice,
  chooseAnswer,
  confirmAnswer,
  confirmation,
  mulligan,
  mulliganAnswer,
} from "../../battle/view/choices"
import { inspect } from "../../battle/view/inspect"
import { NO_PICKS, problems, selectAnswer, selection, toggle } from "../../battle/view/select"
import { useCards } from "./battle-cards"
import type { BattleState } from "./use-battle"

export function PracticeControls({
  source,
  battle,
}: {
  readonly source: BattleSource
  readonly battle: BattleState
}) {
  const { t } = useTranslation()
  const cards = useCards()
  const [submitting, setSubmitting] = useState<Question["id"]>()
  const [refused, setRefused] = useState<Question["id"]>()
  const { input, question, board } = battle
  if (input?.kind === "finished") {
    return (
      <p role="status" className="bp-practice-outcome">
        {input.winner === null
          ? t("play.practice.draw")
          : input.winner === source.seat
            ? t("play.practice.won")
            : t("play.practice.lost")}
      </p>
    )
  }
  if (input?.kind === "engine_fault") return <p role="alert">{t("play.stopped")}</p>
  if (question === null || board === null) return null

  const buttons: { label: string; command: Command }[] = []
  const choose = choice(question, source.seat)
  if (question.use === "first_player" && choose) {
    for (const option of choose.options) {
      if (option.value.kind !== "seat") continue
      buttons.push({
        label:
          option.value.player === source.seat
            ? t("play.practice.first")
            : t("play.practice.second"),
        command: chooseAnswer(choose, [option.id]),
      })
    }
  }
  if (question.use === "pending_order" && choose?.bounds.max === 1) {
    for (const option of choose.options) {
      if (option.value.kind !== "occurrence") continue
      const { source: origin } = option.value
      const card =
        origin.kind === "text"
          ? origin.text.card
          : origin.face !== null
            ? { printing_id: origin.printing_id, face: origin.face }
            : null
      const name =
        card === null
          ? t("play.card.unknown")
          : (cards.find(card)?.name.text ?? t("play.card.unknown"))
      buttons.push({
        label: t("play.practice.resolve", { name }),
        command: chooseAnswer(choose, [option.id]),
      })
    }
  }
  const opening = mulligan(question, source.seat)
  if (opening) {
    buttons.push({ label: t("play.practice.keep"), command: mulliganAnswer(opening, null) })
  }
  const confirm = confirmation(question, source.seat)
  if (confirm) {
    for (const accept of confirm.allowed) {
      buttons.push({
        label: accept ? t("play.practice.accept") : t("play.practice.decline"),
        command: confirmAnswer(confirm, accept),
      })
    }
  }
  const select = selection(question, source.seat)
  if (select && problems(select, NO_PICKS).length === 0) {
    buttons.push({ label: t("play.practice.pass"), command: selectAnswer(select, NO_PICKS) })
  }
  if (select && (select.selection.bounds.max === 1 || question.use === "end_guard")) {
    for (const candidate of select.selection.candidates) {
      const picks = toggle(select, NO_PICKS, candidate.id)
      if (problems(select, picks).length > 0) continue
      const subject = candidate.subject
      const name =
        subject?.kind === "card"
          ? (cards.find(subject.card)?.name.text ?? t("play.card.unknown"))
          : subject == null
            ? t("play.card.unknown")
            : subject.player === source.seat
              ? t("play.side.self")
              : t("play.side.opponent")
      buttons.push({
        label: t("play.practice.select", { name }),
        command: selectAnswer(select, picks),
      })
    }
  }
  const nameOf = (holder: Parameters<typeof inspect>[1]): string => {
    const face = inspect(board, holder)?.card.face
    return (face === undefined ? undefined : cards.find(face)?.name.text) ?? t("play.card.unknown")
  }
  const main = actions(question, source.seat)
  if (main) {
    for (const [holder, plays] of main.play) {
      const name = nameOf(holder)
      for (const play of plays) {
        buttons.push({ label: t("play.practice.play", { name }), command: act(main, play.option) })
      }
    }
    for (const [holder, attack] of main.attack) {
      const name = nameOf(holder)
      if (attack.leader !== null) {
        buttons.push({
          label: t("play.practice.attack", { name, target: t("play.practice.opponentLeader") }),
          command: act(main, attack.option, attack.leader),
        })
      }
      for (const [target, option] of attack.cards) {
        buttons.push({
          label: t("play.practice.attack", { name, target: nameOf(target) }),
          command: act(main, attack.option, option),
        })
      }
    }
    if (main.endPhase !== null) {
      buttons.push({ label: t("play.turn.end"), command: act(main, main.endPhase) })
    }
  }
  const window = quick(question, source.seat)
  if (window) {
    buttons.push({ label: t("play.practice.pass"), command: actQuick(window, null) })
  }
  const submit = async (command: Command) => {
    setSubmitting(question.id)
    setRefused(undefined)
    try {
      const receipt = await source.submit(command)
      if (receipt.result === "applied") return
    } catch {
      // A stopped or closed channel may never deliver a receipt.
    }
    setSubmitting(undefined)
    setRefused(question.id)
  }
  return (
    <div
      role="group"
      aria-label={t("play.practice.controls")}
      className="flex flex-wrap justify-end gap-1"
    >
      {buttons.map(({ label, command }) => (
        <button
          key={JSON.stringify(command)}
          className="bp-chip-link"
          type="button"
          disabled={submitting === question.id}
          onClick={() => void submit(command)}
        >
          {label}
        </button>
      ))}
      {buttons.length === 0 && <span role="status">{t("play.practice.unsupported")}</span>}
      {refused === question.id && <span role="status">{t("play.practice.refused")}</span>}
    </div>
  )
}
