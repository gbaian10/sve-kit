// @vitest-environment node
import { describe, expect, it } from "vitest"

import {
  type BattleUpdate,
  type Command,
  type HostMessage,
  type Question,
  validateHostMessage,
} from "../contract"
import type { BattleSource } from "../source"
import { act, type Actions, actions } from "../view/actions"
import { type Scene, SCENES } from "./scenes"
import { scriptedTable } from "./scripted"

function scene(id: Scene["id"]): Scene {
  const found = SCENES.find((entry) => entry.id === id)
  if (found === undefined) throw new Error(`no scene ${id}`)
  return found
}

/** Everything one seat has been sent so far, and what its last update says. */
function follow(source: BattleSource) {
  const messages: HostMessage[] = []
  source.listen((message) => messages.push(message))
  const last = (): BattleUpdate => {
    const update = messages.filter((message) => message.kind === "update").at(-1)
    if (update === undefined) throw new Error("no update yet")
    return update
  }
  const question = (): Question | null => {
    const { input } = last()
    return input.kind === "question" ? input.question : null
  }
  const acts = (): Actions => {
    const open = actions(question(), source.seat)
    if (open === null) throw new Error("no main-phase question is open")
    return open
  }
  return { messages, last, question, acts }
}

const need = <T>(value: T | null | undefined): T => {
  if (value === null || value === undefined) throw new Error("the question does not offer this")
  return value
}

/** The turn scene's steps, each picked from what the open question offers. */
const STEPS = {
  play: (acts: Actions): Command => act(acts, need([...acts.play.values()][0]?.[0]).option),
  /** The attacker that may reach the leader attacks it. */
  leader: (acts: Actions): Command => {
    const attack = need([...acts.attack.values()].find((each) => each.leader !== null))
    return act(acts, attack.option, attack.leader)
  },
  /** The same attacker attacks the engaged follower instead. */
  trade: (acts: Actions): Command => {
    const attack = need([...acts.attack.values()].find((each) => each.leader !== null))
    return act(acts, attack.option, need([...attack.cards.values()][0]))
  },
  /** The attacker that may only reach followers attacks the engaged one. */
  rush: (acts: Actions): Command => {
    const attack = need([...acts.attack.values()].find((each) => each.leader === null))
    return act(acts, attack.option, need([...attack.cards.values()][0]))
  },
  end: (acts: Actions): Command => act(acts, need(acts.endPhase)),
}

async function played(steps: readonly (keyof typeof STEPS)[]) {
  const seats = scriptedTable(scene("turn"))
  const own = follow(seats.p1)
  const other = follow(seats.p2)
  for (const step of steps) {
    const receipt = await seats.p1.submit(STEPS[step](own.acts()))
    expect(receipt).toMatchObject({ kind: "receipt", result: "applied", rejection: null })
  }
  return { seats, own, other }
}

describe("scriptedTable", () => {
  it("replays everything delivered so far to every listener until the source is closed", async () => {
    const turn = scene("turn")
    const { p1, p2 } = scriptedTable(turn)
    const early = follow(p2)
    expect(early.messages).toEqual(turn.script.p2)
    await p1.submit(STEPS.play(follow(p1).acts()))
    // A listener that comes late starts from the same messages, the new update included.
    const late = follow(p2)
    expect(late.messages).toHaveLength(turn.script.p2.length + 1)
    expect(late.messages).toEqual(early.messages)
    p2.close()
    expect(follow(p2).messages).toEqual([])
    await expect(p2.submit({ kind: "concede" })).rejects.toThrow("closed")
  })

  it("plays the turn to its end, every answer moving both seats' tables on", async () => {
    const { seats, own, other } = await played([])
    const [card] = [...own.acts().play.keys()]
    await seats.p1.submit(STEPS.play(own.acts()))
    const self = () => own.last().observation.players[0]
    const opposing = () => own.last().observation.players[1]
    // The card left the hand for the field under the same handle, at its quoted price.
    expect(self().zones.field.cards.at(-1)?.object).toBe(card)
    expect(self().zones.hand.count).toBe(2)
    expect(self().pp).toBe(1)
    expect(own.acts().play.size).toBe(0)
    expect(other.last().observation.players[0].zones.field.count).toBe(3)

    await seats.p1.submit(STEPS.leader(own.acts()))
    expect(opposing().life).toBe(15)
    // Having attacked, it is offered no second attack.
    expect([...own.acts().attack.values()].every((attack) => attack.leader === null)).toBe(true)

    const [target] = [...need([...own.acts().attack.values()][0]).cards.keys()]
    await seats.p1.submit(STEPS.rush(own.acts()))
    // Both fell: each lies on its owner's cemetery under the handle it had on the field.
    expect(opposing().zones.field.cards.map((member) => member.object)).not.toContain(target)
    expect(opposing().zones.cemetery.cards.at(-1)?.object).toBe(target)
    expect(self().zones.field.count).toBe(2)
    expect(own.acts().attack.size).toBe(0)

    await seats.p1.submit(STEPS.end(own.acts()))
    expect(own.last().update_seq).toBe(4)
    expect(own.last().input).toMatchObject({ kind: "waiting", actor: "p2" })
    expect(other.question()).toMatchObject({ actor: "p2" })
    expect(other.last().observation.turn).toMatchObject({ active: "p2", elapsed_turns: [5, 6] })
    for (const message of [...own.messages, ...other.messages])
      expect(validateHostMessage(message)).toBe(true)
  })

  it("takes the turn's actions in any order", async () => {
    const one = await played(["play", "leader", "rush", "end"])
    const other = await played(["rush", "play", "end"])
    const third = await played(["rush", "leader", "play", "end"])
    expect(third.own.last().observation).toEqual(one.own.last().observation)
    expect(third.other.last().observation).toEqual(one.other.last().observation)
    // The turn may end with an attack still unmade.
    expect(other.own.last().observation.players[1].life).toBe(18)
  })

  it("offers only what is left after an attack", async () => {
    // The engaged follower fell to the first attacker, which took its damage and is engaged: the
    // follower with rush has nothing left to attack.
    const traded = await played(["trade"])
    expect(traded.own.acts().attack.size).toBe(0)
    const [attacker] = traded.own.last().observation.players[0].zones.field.cards
    expect(traded.own.last().observation.objects[need(attacker).object]?.state).toMatchObject({
      posture: "acted",
      stats: { attack: 3, life: 1 },
    })
    // It fell to the follower with rush instead: the other attacker still reaches the leader.
    const rushed = await played(["rush"])
    const [left] = [...rushed.own.acts().attack.values()]
    expect(left?.leader).not.toBeNull()
    expect(left?.cards.size).toBe(0)
  })

  it("refuses an answer the open question does not offer, and changes nothing", async () => {
    const { seats, own, other } = await played(["play"])
    const acts = own.acts()
    const attack = need([...acts.attack.values()].find((each) => each.leader !== null))
    const sent = [own.messages.length, other.messages.length]
    const refused: Command[] = [
      // A question that is no longer open, an option nobody offered, an attack without its
      // recipient, a recipient for something that is no attack, another attacker's recipient.
      act({ ...acts, question: "q0" }, need(acts.endPhase)),
      act(acts, "play:a:fanatic"),
      act(acts, attack.option),
      act(acts, need(acts.endPhase), attack.leader),
      act(acts, attack.option, "attack:a:priest>a:fighter"),
      { kind: "answer", question_id: acts.question, answer: { kind: "quick", option: null } },
      { kind: "concede" },
    ]
    for (const command of refused)
      expect(await seats.p1.submit(command)).toMatchObject({
        result: "rejected",
        rejection: "invalid_submission",
      })
    // The seat that is asked nothing may answer nothing, even with the other seat's options.
    expect(await seats.p2.submit(STEPS.end(acts))).toMatchObject({ result: "rejected" })
    expect([own.messages.length, other.messages.length]).toEqual(sent)
    // The question is still open: the right answer is taken.
    expect(await seats.p1.submit(STEPS.end(acts))).toMatchObject({ result: "applied" })
    expect(await seats.p1.submit(STEPS.end(acts))).toMatchObject({ result: "rejected" })
  })

  it("refuses every answer in a scene that only shows a table", async () => {
    const seats = scriptedTable(scene("preset"))
    const own = follow(seats.p1)
    // The question is there, for what it says may attack; the script goes nowhere from it.
    expect(own.acts().attack.size).toBeGreaterThan(0)
    expect(await seats.p1.submit(STEPS.end(own.acts()))).toMatchObject({ result: "rejected" })
    expect(own.messages).toHaveLength(2)
  })

  it.each([
    ["finished", { kind: "finished", reason: "defeat", winner: "p1" }],
    ["fault", { kind: "engine_fault", can_concede: true }],
  ] as const)("asks nothing once the game is %s", async (id, input) => {
    const seats = scriptedTable(scene(id))
    for (const seat of [seats.p1, seats.p2]) {
      const seen = follow(seat)
      expect(seen.last().input).toEqual(input)
      expect(seen.question()).toBeNull()
      expect(await seat.submit({ kind: "concede" })).toMatchObject({ result: "rejected" })
    }
  })

  it("takes no answer after the host has stopped", async () => {
    const seats = scriptedTable(scene("halted"))
    const own = follow(seats.p1)
    expect(own.messages.at(-1)).toEqual({ kind: "host_error", format: 1, reason: "projection" })
    // The update before the error still carries a question; it may no longer be answered.
    expect(await seats.p1.submit(STEPS.end(own.acts()))).toMatchObject({ result: "rejected" })
  })
})
