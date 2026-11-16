// @vitest-environment node
import { describe, expect, it } from "vitest"

import {
  type BattleUpdate,
  type Command,
  type HostMessage,
  type Question,
  type Receipt,
  validateHostMessage,
} from "../contract"
import type { BattleSource } from "../source"
import { act, type Actions, actions, actQuick, quick } from "../view/actions"
import {
  choice,
  chooseAnswer,
  confirmAnswer,
  confirmation,
  mulligan,
  mulliganAnswer,
} from "../view/choices"
import { NO_PICKS, selectAnswer, selection, toggle } from "../view/select"
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

/** Lets what an applied command changed arrive: it comes in a later task than the receipt. */
const arrive = (): Promise<void> => new Promise((resolve) => setTimeout(resolve, 0))

/** One command sent and, once its receipt is in, everything it changed delivered. */
async function send(source: BattleSource, command: Command): Promise<Receipt> {
  const receipt = await source.submit(command)
  await arrive()
  return receipt
}

const seqs = (messages: readonly HostMessage[]): number[] =>
  messages.flatMap((message) => (message.kind === "update" ? [message.update_seq] : []))

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
  for (const step of steps)
    expect(await send(seats.p1, STEPS[step](own.acts()))).toMatchObject({
      kind: "receipt",
      result: "applied",
      rejection: null,
    })
  return { seats, own, other }
}

describe("scriptedTable", () => {
  it("replays everything delivered so far to every listener until the source is closed", async () => {
    const turn = scene("turn")
    const { p1, p2 } = scriptedTable(turn)
    const early = follow(p2)
    expect(early.messages).toEqual(turn.script.p2)
    await send(p1, STEPS.play(follow(p1).acts()))
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
    await send(seats.p1, STEPS.play(own.acts()))
    const self = () => own.last().observation.players[0]
    const opposing = () => own.last().observation.players[1]
    // The card left the hand for the field under the same handle, at its quoted price.
    expect(self().zones.field.cards.at(-1)?.object).toBe(card)
    expect(self().zones.hand.count).toBe(2)
    expect(self().pp).toBe(1)
    expect(own.acts().play.size).toBe(0)
    expect(other.last().observation.players[0].zones.field.count).toBe(3)

    await send(seats.p1, STEPS.leader(own.acts()))
    expect(opposing().life).toBe(15)
    // Having attacked, it is offered no second attack.
    expect([...own.acts().attack.values()].every((attack) => attack.leader === null)).toBe(true)

    const [target] = [...need([...own.acts().attack.values()][0]).cards.keys()]
    await send(seats.p1, STEPS.rush(own.acts()))
    // Both fell: each lies on its owner's cemetery under the handle it had on the field.
    expect(opposing().zones.field.cards.map((member) => member.object)).not.toContain(target)
    expect(opposing().zones.cemetery.cards.at(-1)?.object).toBe(target)
    expect(self().zones.field.count).toBe(2)
    expect(own.acts().attack.size).toBe(0)

    await send(seats.p1, STEPS.end(own.acts()))
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
    // The seat that is asked nothing may answer nothing, even with the other seat's options. Each
    // seat numbers its own requests, whatever the other seat sends.
    expect(await seats.p2.submit(STEPS.end(acts))).toMatchObject({
      request_id: "p2-c1",
      result: "rejected",
    })
    await arrive()
    expect([own.messages.length, other.messages.length]).toEqual(sent)
    // The question is still open: the right answer is taken, once.
    expect(await send(seats.p1, STEPS.end(acts))).toMatchObject({
      request_id: "p1-c9",
      result: "applied",
    })
    expect(await send(seats.p1, STEPS.end(acts))).toMatchObject({ result: "rejected" })
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

  it("gives no receipt once the host has stopped", async () => {
    const seats = scriptedTable(scene("halted"))
    const own = follow(seats.p1)
    expect(own.messages.at(-1)).toEqual({ kind: "host_error", format: 1, reason: "projection" })
    // The update before the error still carries a question. Answering it is not refused like a
    // wrong answer, which could be put right: the command fails, as it does on the Worker's channel.
    await expect(seats.p1.submit(STEPS.end(own.acts()))).rejects.toThrow("stopped")
    await expect(seats.p2.submit({ kind: "concede" })).rejects.toThrow("stopped")
  })

  it("gives the receipt before the update the command caused", async () => {
    const { seats, own } = await played([])
    const order: string[] = []
    seats.p1.listen((message) => {
      if (message.kind === "update" && message.update_seq > 0)
        order.push(`update:${String(message.update_seq)}`)
    })
    // Whatever the receipt sets off runs first, a chain of it included.
    await seats.p1
      .submit(STEPS.play(own.acts()))
      .then((receipt) => order.push(`receipt:${receipt.result}`))
      .then(() => order.push("after the receipt"))
    expect(order).toEqual(["receipt:applied", "after the receipt"])
    await arrive()
    expect(order).toEqual(["receipt:applied", "after the receipt", "update:1"])
    // A refused command sets nothing off.
    const refused = await send(seats.p1, act(own.acts(), "play:a:fanatic"))
    expect(refused).toMatchObject({ result: "rejected" })
    expect(order).toHaveLength(3)
  })

  it("takes one command of a seat at a time", async () => {
    const { seats, own, other } = await played([])
    const acts = own.acts()
    const first = seats.p1.submit(STEPS.play(acts))
    // The second never reaches the host, so it uses up no request id and applies nothing.
    await expect(seats.p1.submit(STEPS.end(acts))).rejects.toThrow("still out")
    expect(await first).toMatchObject({ request_id: "p1-c1", result: "applied" })
    await arrive()
    expect(seqs(other.messages)).toEqual([0, 1])
    expect(await send(seats.p1, STEPS.end(own.acts()))).toMatchObject({
      request_id: "p1-c2",
      result: "applied",
    })
  })

  it("lets nothing a listener sends while it is told overtake the update", async () => {
    const { seats, own, other } = await played([])
    // Told of the first update, the seat at once answers the question in it: the other seat has
    // not been told yet, so the command that caused the update is still out.
    let during: Promise<string> | undefined
    seats.p1.listen((message) => {
      if (message.kind === "update" && message.update_seq === 1)
        during = seats.p1.submit(STEPS.end(own.acts())).then(
          (receipt) => receipt.result,
          (reason: unknown) => String(reason),
        )
    })
    const played1 = await send(seats.p1, STEPS.play(own.acts()))
    expect(await during).toMatch("still out")
    // Nothing was applied behind the other seat's back, and its updates came in order.
    expect(seqs(other.messages)).toEqual([0, 1])
    expect(other.last().observation.turn.active).toBe("p1")
    // Both seats told, the same answer is taken, under the next request id.
    const ended = await send(seats.p1, STEPS.end(own.acts()))
    expect([played1.request_id, ended.request_id]).toEqual(["p1-c1", "p1-c2"])
    expect(ended.result).toBe("applied")
    expect(seqs(other.messages)).toEqual([0, 1, 2])
    expect(seqs(own.messages)).toEqual([0, 1, 2])
  })

  it("answers the two seats' commands in the order they were sent", async () => {
    const { seats, own, other } = await played([])
    // Both sent before the host answers either: the first seat's is applied, and the other
    // seat's, which answers nothing it was asked, is refused without holding anything up.
    const first = seats.p1.submit(STEPS.end(own.acts()))
    const second = seats.p2.submit({ kind: "concede" })
    expect(await first).toMatchObject({ request_id: "p1-c1", result: "applied" })
    expect(await second).toMatchObject({ request_id: "p2-c1", result: "rejected" })
    await arrive()
    expect(seqs(other.messages)).toEqual([0, 1])
    expect(other.question()).toMatchObject({ actor: "p2" })
  })

  it("fails a command that is still out when its source closes", async () => {
    const { seats, own, other } = await played([])
    const pending = seats.p1.submit(STEPS.play(own.acts()))
    seats.p1.close()
    await expect(pending).rejects.toThrow("closed")
    await arrive()
    // It never reached the host: the other seat's table stands as it was.
    expect(seqs(other.messages)).toEqual([0])
  })

  it.each([
    ["passed", null, { life: 14, pp: 2, hand: 3 }],
    ["played", "quick:a:quick", { life: 14, pp: 0, hand: 2 }],
  ] as const)(
    "takes a Quick window %s, once, and lets the attack land",
    async (_, option, after) => {
      const seats = scriptedTable(scene("quick"))
      const own = follow(seats.p1)
      const other = follow(seats.p2)
      // The seat that is asked sees the question; the other only that it is waited for.
      const window = need(quick(own.question(), "p1"))
      expect(other.last().input).toMatchObject({ kind: "waiting", actor: "p1" })
      expect(own.last().observation.attack).toMatchObject({
        target: { kind: "leader", player: "p1" },
      })
      expect(await send(seats.p1, actQuick(window, "quick:nothing"))).toMatchObject({
        result: "rejected",
      })
      expect(await send(seats.p1, actQuick(window, option))).toMatchObject({ result: "applied" })
      const self = own.last().observation.players[0]
      expect({ life: self.life, pp: self.pp, hand: self.zones.hand.count }).toEqual(after)
      expect(own.last().observation.attack).toBeNull()
      // The turn goes on: the other seat is asked what to do in its main phase.
      expect(own.last().input).toMatchObject({ kind: "waiting", actor: "p2" })
      expect(other.question()).toMatchObject({ actor: "p2", shape: { kind: "action_window" } })
      expect(await send(seats.p1, actQuick(window, option))).toMatchObject({ result: "rejected" })
      for (const message of [...own.messages, ...other.messages])
        expect(validateHostMessage(message)).toBe(true)
    },
  )

  /** A scene's first seat, what it is asked now, and the selection its picks would answer with. */
  function asking(id: Scene["id"]) {
    const seats = scriptedTable(scene(id))
    const own = follow(seats.p1)
    const other = follow(seats.p2)
    const picking = (...ids: string[]): Command => {
      const asked = need(selection(own.question(), "p1"))
      return selectAnswer(
        asked,
        ids.reduce((picks, id) => toggle(asked, picks, id), NO_PICKS),
      )
    }
    const valid = (): boolean =>
      [...own.messages, ...other.messages].every((message) => validateHostMessage(message))
    return { seats, own, other, picking, valid }
  }
  const applied = { result: "applied" }
  const rejected = { result: "rejected", rejection: "invalid_submission" }

  it("takes one follower of each side for the spell that needs both", async () => {
    const { seats, own, other, picking, valid } = asking("targets")
    expect(other.last().input).toMatchObject({ kind: "waiting", actor: "p1" })
    // One pick short, and two of the same side: neither is the selection the spell asks for.
    expect(await send(seats.p1, picking("own:priest"))).toMatchObject(rejected)
    expect(await send(seats.p1, picking("own:priest", "own:unifier"))).toMatchObject(rejected)
    expect(await send(seats.p1, picking("foe:meimu", "own:priest"))).toMatchObject(applied)
    const [self, opposing] = own.last().observation.players
    // The seat's own follower took one damage; the other fell and lies on its owner's cemetery.
    expect(own.last().observation.objects["a:priest"]?.state?.stats).toEqual({ attack: 2, life: 1 })
    expect(opposing.zones.field.cards.map((member) => member.object)).not.toContain("a:meimu")
    expect(opposing.zones.cemetery.cards.at(-1)?.object).toBe("a:meimu")
    expect({ pp: self.pp, hand: self.zones.hand.count }).toEqual({ pp: 2, hand: 2 })
    // The turn goes on with the main-phase question.
    expect(own.question()?.shape.kind).toBe("action_window")
    expect(valid()).toBe(true)
  })

  it.each([
    [true, ["deck:maimu-1", "deck:miimu"], { pp: 1, field: 4, hand: 2, deck: 22 }],
    [false, ["deck:miimu"], { pp: 5, field: 2, hand: 3, deck: 23 }],
  ] as const)(
    "asks whether to pay the extra cost (%s), then for the search that follows",
    async (paid, found, after) => {
      const { seats, own, picking, valid } = asking("sisters")
      const extra = need(confirmation(own.question(), "p1"))
      expect(await send(seats.p1, confirmAnswer(extra, paid))).toMatchObject(applied)
      // The same table, and the next question of the same effect.
      const search = need(selection(own.question(), "p1")).selection
      expect(search.candidates.map((candidate) => candidate.subject?.kind)).toEqual([
        "card",
        "card",
        "card",
      ])
      expect(search.group_assignment).toBe(paid ? "explicit" : "existence")
      expect(search.bounds.max).toBe(paid ? 3 : 1)
      // Two of one name: one group too full where groups are named, one pick too many elsewhere.
      const both: Command = {
        kind: "answer",
        question_id: "search",
        answer: {
          kind: "select",
          selected: ["deck:maimu-1", "deck:maimu-2"],
          groups: paid
            ? [
                { group: 0, selected: ["deck:maimu-1", "deck:maimu-2"] },
                { group: 1, selected: [] },
                { group: 2, selected: [] },
              ]
            : null,
          roles: [],
        },
      }
      expect(await send(seats.p1, both)).toMatchObject(rejected)
      if (paid) {
        // The groups have to be named, every one of them, the empty one too.
        const unnamed = picking(...found)
        if (unnamed.kind !== "answer" || unnamed.answer.kind !== "select") throw new Error("select")
        for (const groups of [null, unnamed.answer.groups?.slice(0, 2) ?? null])
          expect(
            await send(seats.p1, { ...unnamed, answer: { ...unnamed.answer, groups } }),
          ).toMatchObject(rejected)
      }
      expect(await send(seats.p1, picking(...found))).toMatchObject(applied)
      const [self] = own.last().observation.players
      expect({
        pp: self.pp,
        field: self.zones.field.count,
        hand: self.zones.hand.count,
        deck: self.zones.deck.count,
      }).toEqual(after)
      expect(own.question()?.shape.kind).toBe("action_window")
      expect(valid()).toBe(true)
    },
  )

  it("lets the search find nothing", async () => {
    const { seats, own, picking } = asking("sisters")
    await send(seats.p1, confirmAnswer(need(confirmation(own.question(), "p1")), true))
    expect(await send(seats.p1, picking())).toMatchObject(applied)
    const [self] = own.last().observation.players
    expect({ field: self.zones.field.count, deck: self.zones.deck.count }).toEqual({
      field: 2,
      deck: 24,
    })
  })

  it("discards down to the hand limit in the order the cards are named", async () => {
    const { seats, own, other, picking, valid } = asking("discard")
    expect(await send(seats.p1, picking("held-3"))).toMatchObject(rejected)
    expect(await send(seats.p1, picking("held-3", "held-0"))).toMatchObject(applied)
    const [self] = own.last().observation.players
    expect(self.zones.hand.count).toBe(7)
    expect(self.zones.cemetery.cards.slice(-2).map((member) => member.object)).toEqual([
      "a:held-3",
      "a:held-0",
    ])
    // The turn is over: the other seat is asked, this one waits.
    expect(own.last().input).toMatchObject({ kind: "waiting", actor: "p2" })
    expect(other.question()).toMatchObject({ actor: "p2" })
    expect(valid()).toBe(true)
  })

  it.each([
    ["mode:1", { maxPp: 6, hand: 2, deck: 23 }],
    ["mode:2", { maxPp: 5, hand: 3, deck: 22 }],
  ] as const)("takes one mode of the spell (%s)", async (mode, after) => {
    const { seats, own, valid } = asking("modes")
    const asked = need(choice(own.question(), "p1"))
    expect(await send(seats.p1, chooseAnswer(asked, ["mode:1", "mode:2"]))).toMatchObject(rejected)
    expect(await send(seats.p1, chooseAnswer(asked, []))).toMatchObject(rejected)
    expect(await send(seats.p1, chooseAnswer(asked, [mode]))).toMatchObject(applied)
    const [self] = own.last().observation.players
    expect({
      maxPp: self.max_pp,
      hand: self.zones.hand.count,
      deck: self.zones.deck.count,
    }).toEqual(after)
    expect(self.pp).toBe(3)
    expect(valid()).toBe(true)
  })

  it("takes who plays first, and the opening hand kept or returned whole", async () => {
    const first = asking("firstPlayer")
    const seat = need(choice(first.own.question(), "p1"))
    expect(await send(first.seats.p1, chooseAnswer(seat, ["seat:p2"]))).toMatchObject(applied)
    expect(first.own.last().observation.turn.first_player).toBe("p2")
    expect(first.own.last().input).toMatchObject({ kind: "waiting", actor: "p2" })
    expect(first.valid()).toBe(true)

    const kept = asking("mulligan")
    const hand = need(mulligan(kept.own.question(), "p1"))
    const before = kept.own.last().observation.players[0].zones.hand.cards
    // Part of the hand cannot be returned.
    expect(
      await send(
        kept.seats.p1,
        mulliganAnswer(
          hand,
          hand.hand.slice(1).map((card) => card.id),
        ),
      ),
    ).toMatchObject(rejected)
    expect(await send(kept.seats.p1, mulliganAnswer(hand, null))).toMatchObject(applied)
    expect(kept.own.last().observation.players[0].zones.hand.cards).toEqual(before)

    const redrawn = asking("mulligan")
    const order = need(mulligan(redrawn.own.question(), "p1")).hand.map((card) => card.id)
    expect(await send(redrawn.seats.p1, mulliganAnswer(hand, [...order].reverse()))).toMatchObject(
      applied,
    )
    const after = redrawn.own.last().observation.players[0].zones.hand
    expect(after.count).toBe(4)
    // Four other cards under handles of their own.
    expect(after.cards.some((member) => before.some((old) => old.object === member.object))).toBe(
      false,
    )
    expect(redrawn.valid()).toBe(true)
  })
})
