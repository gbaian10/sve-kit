import { act, renderHook } from "@testing-library/react"
import { expect, it } from "vitest"

import { SCENES } from "../../battle/fixtures/scenes"
import { scriptedTable } from "../../battle/fixtures/scripted"
import { act as answer, type Actions, actions } from "../../battle/view/actions"
import { useBattle } from "./use-battle"

const need = <T,>(value: T | null | undefined): T => {
  if (value === null || value === undefined) throw new Error("the question does not offer this")
  return value
}

it("follows the open question, what happens next and where the cards lie", async () => {
  const scene = need(SCENES.find((entry) => entry.id === "turn"))
  const seats = scriptedTable(scene)
  const { result } = renderHook(() => useBattle(seats.p1, scene.slots))
  const acts = (): Actions => need(actions(result.current.question, "p1"))
  expect(result.current.input?.kind).toBe("question")
  // Laid as the scene arranges its first table.
  const [unifier, priest] = result.current.placement.p1.field
  expect([unifier, priest].every((handle) => typeof handle === "string")).toBe(true)
  expect(result.current.placement.p2.field.map((handle) => handle !== null)).toEqual([
    true,
    true,
    true,
    false,
    false,
  ])

  // The follower with rush and the engaged follower both fall: what stays does not move.
  const rush = need([...acts().attack.values()].find((attack) => attack.leader === null))
  await act(() => seats.p1.submit(answer(acts(), rush.option, need([...rush.cards.values()][0]))))
  expect(result.current.placement.p1.field).toEqual([unifier, null, null, null, null])
  expect(result.current.placement.p2.field.map((handle) => handle !== null)).toEqual([
    true,
    false,
    true,
    false,
    false,
  ])

  // The card played arrives in the first free slot, which the fallen follower left.
  const [card, plays] = need([...acts().play][0])
  await act(() => seats.p1.submit(answer(acts(), need(plays[0]).option)))
  expect(result.current.placement.p1.field).toEqual([unifier, card, null, null, null])
  expect(result.current.board?.players[0].pp).toBe(1)

  await act(() => seats.p1.submit(answer(acts(), need(acts().endPhase))))
  expect(result.current.question).toBeNull()
  expect(result.current.input).toMatchObject({ kind: "waiting", actor: "p2" })
  expect(result.current.stopped).toBeNull()
})

it("says what the last update leaves to happen when nothing is asked", () => {
  const over = scriptedTable(need(SCENES.find((entry) => entry.id === "finished")))
  const finished = renderHook(() => useBattle(over.p1))
  expect(finished.result.current.input).toEqual({
    kind: "finished",
    reason: "defeat",
    winner: "p1",
  })
  expect(finished.result.current.question).toBeNull()
  const halted = scriptedTable(need(SCENES.find((entry) => entry.id === "halted")))
  const stopped = renderHook(() => useBattle(halted.p1))
  // The table stays as it last stood; the error is what ends the interaction.
  expect(stopped.result.current.board).not.toBeNull()
  expect(stopped.result.current.stopped).toMatchObject({ kind: "host_error", reason: "projection" })
})
