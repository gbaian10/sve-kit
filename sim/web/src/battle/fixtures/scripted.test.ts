// @vitest-environment node
import { describe, expect, it } from "vitest"

import {
  type BattleUpdate,
  type Command,
  type HostMessage,
  type Question,
  type QuestionShape,
  type Receipt,
  validateHostMessage,
} from "../contract"
import type { BattleSource } from "../source"
import { act, type Actions, actions, actQuick, quick } from "../view/actions"
import {
  distributeAnswer,
  distribution,
  integer,
  integerAnswer,
  NO_PILES,
  orderAnswer,
  ordering,
  partition,
  partitionAnswer,
  put,
} from "../view/arrange"
import {
  choice,
  chooseAnswer,
  confirmAnswer,
  confirmation,
  mulligan,
  mulliganAnswer,
} from "../view/choices"
import {
  dice,
  diceAnswer,
  driveTrigger,
  driveTriggerAnswer,
  evolutionPayment,
  evolutionPaymentAnswer,
  resourcePayment,
  resourcePaymentAnswer,
} from "../view/payments"
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
      const { seats, own, other, picking, valid } = asking("sisters")
      const extra = need(confirmation(own.question(), "p1"))
      const before = [...other.messages]
      expect(await send(seats.p1, confirmAnswer(extra, paid))).toMatchObject(applied)
      // The answer changed nothing the other seat can see: that seat is sent nothing, and its
      // sequence does not move. Only the seat that answered has a new update.
      expect(other.messages).toEqual(before)
      expect(seqs(other.messages)).toEqual([0])
      expect(seqs(own.messages)).toEqual([0, 1])
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
      // The search changes the table for both seats: each is told under its own next number,
      // the seat that answered after its receipt.
      const order: string[] = []
      seats.p1.listen((message) => {
        if (message.kind === "update" && message.update_seq > 1)
          order.push(`p1 update ${String(message.update_seq)}`)
      })
      seats.p2.listen((message) => {
        if (message.kind === "update" && message.update_seq > 0)
          order.push(`p2 update ${String(message.update_seq)}`)
      })
      await seats.p1.submit(picking(...found)).then((receipt) => order.push(receipt.result))
      await arrive()
      expect(order).toEqual(["applied", "p1 update 2", "p2 update 1"])
      const [self] = own.last().observation.players
      expect({
        pp: self.pp,
        field: self.zones.field.count,
        hand: self.zones.hand.count,
        deck: self.zones.deck.count,
      }).toEqual(after)
      // Both seats see the same public table again.
      const [theirs] = other.last().observation.players
      expect({
        pp: theirs.pp,
        field: theirs.zones.field.count,
        hand: theirs.zones.hand.count,
      }).toEqual({ pp: after.pp, field: after.field, hand: after.hand })
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
    // Nobody is the active seat before the first player is chosen.
    const undecided = { active: null, first_player: null, phase: null }
    expect(first.own.last().observation.turn).toMatchObject(undecided)
    expect(first.other.last().observation.turn).toMatchObject(undecided)
    expect(await send(first.seats.p1, chooseAnswer(seat, ["seat:p2"]))).toMatchObject(applied)
    // Chosen, that seat is the active one; the opening goes on, so there is no phase yet.
    const decided = { active: "p2", first_player: "p2", phase: null }
    expect(first.own.last().observation.turn).toMatchObject(decided)
    expect(first.other.last().observation.turn).toMatchObject(decided)
    expect(first.own.last().input).toMatchObject({ kind: "waiting", actor: "p2" })
    expect(first.valid()).toBe(true)

    const kept = asking("mulligan")
    const hand = need(mulligan(kept.own.question(), "p1"))
    const opening = { active: "p1", first_player: "p1", phase: null }
    expect(kept.own.last().observation.turn).toMatchObject(opening)
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
    expect(kept.own.last().observation.turn).toMatchObject(opening)

    const redrawn = asking("mulligan")
    const order = need(mulligan(redrawn.own.question(), "p1")).hand.map((card) => card.id)
    expect(await send(redrawn.seats.p1, mulliganAnswer(hand, [...order].reverse()))).toMatchObject(
      applied,
    )
    const after = redrawn.own.last().observation.players[0].zones.hand
    expect(redrawn.own.last().observation.turn).toMatchObject(opening)
    expect(after.count).toBe(4)
    // Four other cards under handles of their own.
    expect(after.cards.some((member) => before.some((old) => old.object === member.object))).toBe(
      false,
    )
    expect(redrawn.valid()).toBe(true)
  })

  it("ends a question for good once its answer is taken, even when the next one reads the same", async () => {
    // The scene asks its first question a second time: the same table, the same words, another id.
    const sisters = scene("sisters")
    const first = sisters.script.p1.find((message) => message.kind === "update")?.input
    if (first?.kind !== "question") throw new Error("the scene asks its first seat")
    const again: Scene = {
      ...sisters,
      after: (answers) => {
        const [, ...rest] = answers
        const same = sisters.after(answers.slice(0, 1))
        if (same === null) return null
        return rest.length === 0
          ? {
              table: same.table,
              input: { kind: "question", question: { ...first.question, id: "successor" } },
            }
          : sisters.after(rest)
      },
    }
    const seats = scriptedTable(again)
    const own = follow(seats.p1)
    const other = follow(seats.p2)
    const before = [...other.messages]
    const old = need(confirmation(own.question(), "p1"))
    expect(await send(seats.p1, confirmAnswer(old, true))).toMatchObject({ result: "applied" })
    // The seat that answered is told: the question it now has is a new one.
    expect(seqs(own.messages)).toEqual([0, 1])
    const next = need(confirmation(own.question(), "p1"))
    expect(next.question).toBe("successor")
    expect({ ...next, question: "" }).toEqual({ ...old, question: "" })
    // The other seat saw nothing of it.
    expect(other.messages).toEqual(before)
    // The answered question does not come back to life, however alike the new one is.
    expect(await send(seats.p1, confirmAnswer(old, true))).toMatchObject({
      result: "rejected",
      rejection: "invalid_submission",
    })
    expect(seqs(own.messages)).toEqual([0, 1])
    // The new question is the one that can be answered.
    expect(await send(seats.p1, confirmAnswer(next, false))).toMatchObject({ result: "applied" })
    expect(seqs(own.messages)).toEqual([0, 1, 2])
    expect(own.question()?.id).toBe("search")
    expect(other.messages).toEqual(before)
  })

  it("leaves the other seat's open question as it is when that seat is told of a change", async () => {
    // Both seats have a question open. The first answers its own, which changes the table for
    // both; the script then hands the second seat the question it already has, under another id.
    const sisters = scene("sisters")
    const [ready, first] = sisters.script.p1
    if (ready === undefined || first?.kind !== "update" || first.input.kind !== "question")
      throw new Error("the scene asks its first seat")
    const theirs: Question = {
      ...first.input.question,
      id: "theirs",
      actor: "p2",
      shape: { kind: "confirm", allowed: [true, false], subject: null, origin: null },
    }
    const [otherReady, otherFirst] = sisters.script.p2
    if (otherReady === undefined || otherFirst?.kind !== "update") throw new Error("two seats")
    const both: Scene = {
      ...sisters,
      script: {
        p1: sisters.script.p1,
        p2: [otherReady, { ...otherFirst, input: { kind: "question", question: theirs } }],
      },
      after: (answers) => {
        const same = sisters.after(answers.slice(0, 1))
        if (same === null || answers.length > 2) return null
        const [self, opposing] = same.table.seats
        return {
          table: {
            ...same.table,
            seats: [{ ...self, life: self.life - answers.length }, opposing],
          },
          input: {
            kind: "question",
            question: { ...theirs, id: `theirs-${String(answers.length)}` },
          },
        }
      },
    }
    const seats = scriptedTable(both)
    const own = follow(seats.p1)
    const other = follow(seats.p2)
    expect(other.question()?.id).toBe("theirs")
    await send(seats.p1, confirmAnswer(need(confirmation(own.question(), "p1")), true))
    // The second seat is told of the change, under its own next number, and is still asked the
    // question it had: the first seat's answer ended nothing of the second's.
    expect(seqs(other.messages)).toEqual([0, 1])
    expect(other.last().observation.players[0].life).toBe(16)
    expect(other.question()).toEqual(theirs)
    // The first seat is asked nothing now and waits for the second.
    expect(own.last().input).toMatchObject({ kind: "waiting", actor: "p2" })
    // The id the script made up is no question of this game; the one the seat holds is.
    const held = need(confirmation(other.question(), "p2"))
    expect(
      await send(seats.p2, confirmAnswer({ ...held, question: "theirs-1" }, true)),
    ).toMatchObject({ result: "rejected" })
    expect(await send(seats.p2, confirmAnswer(held, true))).toMatchObject({ result: "applied" })
    // Answered, it is over: what the seat is asked next carries another id.
    expect(other.question()?.id).toBe("theirs-2")
    expect(await send(seats.p2, confirmAnswer(held, true))).toMatchObject({ result: "rejected" })
  })

  /** A scene whose opening question, put to the first seat, has its shape changed. */
  function reshaped<K extends QuestionShape["kind"]>(
    id: Scene["id"],
    kind: K,
    change: (shape: Extract<QuestionShape, { kind: K }>) => QuestionShape,
  ): Scene {
    const original = scene(id)
    const [ready, first] = original.script.p1
    if (ready === undefined || first?.kind !== "update" || first.input.kind !== "question")
      throw new Error("the scene asks its first seat")
    const { question } = first.input
    if (question.shape.kind !== kind) throw new Error(`the scene does not ask a ${kind}`)
    const shape = change(question.shape as Extract<QuestionShape, { kind: K }>)
    return {
      ...original,
      script: {
        ...original.script,
        p1: [ready, { ...first, input: { kind: "question", question: { ...question, shape } } }],
      },
    }
  }

  /** What the first seat's screen needs of its own side after an answer. */
  const side = (update: BattleUpdate) => {
    const [self] = update.observation.players
    return {
      pp: self.pp,
      ep: self.ep,
      hand: self.zones.hand.count,
      deck: self.zones.deck.count,
    }
  }
  const stats = (update: BattleUpdate, handle: string) =>
    update.observation.objects[handle]?.state?.stats ?? null

  it("takes an order that names every item once", async () => {
    const { seats, own, other, valid } = asking("order")
    const asked = need(ordering(own.question(), "p1"))
    expect(other.last().input).toMatchObject({ kind: "waiting", actor: "p1" })
    const before = side(own.last())
    for (const order of [
      ["top:2", "top:1"],
      ["top:1", "top:1", "top:2"],
      ["top:1", "top:2", "x"],
    ])
      expect(await send(seats.p1, orderAnswer(asked, order))).toMatchObject(rejected)
    expect(await send(seats.p1, orderAnswer(asked, ["top:3", "top:1", "top:2"]))).toMatchObject(
      applied,
    )
    // The cards went under the deck: the table reads as before, and the turn goes on.
    expect(side(own.last())).toEqual(before)
    expect(own.question()?.shape.kind).toBe("action_window")
    expect(await send(seats.p1, orderAnswer(asked, ["top:3", "top:1", "top:2"]))).toMatchObject(
      rejected,
    )
    expect(valid()).toBe(true)
  })

  it("takes piles that hold every item once, each pile within its size", async () => {
    const { seats, own, valid } = asking("partition")
    const asked = need(partition(own.question(), "p1"))
    const piled = (...placed: (readonly [string, number])[]) =>
      placed.reduce((piles, [item, bucket]) => put(asked, piles, item, bucket), NO_PILES)
    const before = side(own.last())
    // Two for the hand, which takes one; and an item left on no pile.
    for (const piles of [
      piled(["top:1", 0], ["top:2", 0], ["top:3", 1]),
      piled(["top:1", 0], ["top:2", 1]),
    ])
      expect(await send(seats.p1, partitionAnswer(asked, piles))).toMatchObject(rejected)
    // What the readers would never build: an item on two piles, a pile left out, a pile twice.
    const answering = (buckets: { bucket: number; items: string[] }[]): Command => ({
      kind: "answer",
      question_id: asked.question,
      answer: { kind: "partition", buckets },
    })
    for (const buckets of [
      [
        { bucket: 0, items: ["top:1"] },
        { bucket: 1, items: ["top:1", "top:2", "top:3"] },
      ],
      [{ bucket: 1, items: ["top:2", "top:3"] }],
      [
        { bucket: 0, items: ["top:1"] },
        { bucket: 0, items: ["top:2", "top:3"] },
      ],
    ])
      expect(await send(seats.p1, answering(buckets))).toMatchObject(rejected)
    const piles = piled(["top:3", 1], ["top:2", 0], ["top:1", 1])
    expect(await send(seats.p1, partitionAnswer(asked, piles))).toMatchObject(applied)
    // The one kept is in the hand; the deck is one card shorter, the other two back under it.
    expect(side(own.last())).toEqual({ ...before, hand: before.hand + 1, deck: before.deck - 1 })
    expect(own.last().observation.players[0].zones.hand.cards.at(-1)?.object).toBe("a:looked-2")
    expect(valid()).toBe(true)
  })

  it("takes amounts that add up to the total, one for every recipient", async () => {
    const { seats, own, valid } = asking("distribute")
    const asked = need(distribution(own.question(), "p1"))
    const dealing = (...shares: (readonly [string, number])[]) =>
      distributeAnswer(asked, new Map(shares))
    expect(await send(seats.p1, dealing(["foe:maimu", 2]))).toMatchObject(rejected)
    expect(await send(seats.p1, dealing(["foe:maimu", 2], ["foe:meimu", 2]))).toMatchObject(
      rejected,
    )
    // A recipient left out, and one named twice: the answer names each once.
    const answering = (shares: { option: string; amount: number }[]): Command => ({
      kind: "answer",
      question_id: asked.question,
      answer: { kind: "distribute", shares },
    })
    for (const shares of [
      [
        { option: "foe:maimu", amount: 3 },
        { option: "foe:meimu", amount: 0 },
      ],
      [
        { option: "foe:maimu", amount: 1 },
        { option: "foe:maimu", amount: 2 },
        { option: "foe:archer", amount: 0 },
      ],
    ])
      expect(await send(seats.p1, answering(shares))).toMatchObject(rejected)
    expect(await send(seats.p1, dealing(["foe:maimu", 2], ["foe:meimu", 1]))).toMatchObject(applied)
    const [, opposing] = own.last().observation.players
    // Two damage fells the first, one left the second with one life, the third was not touched.
    expect(opposing.zones.field.cards.map((member) => member.object)).toEqual([
      "a:meimu",
      "a:archer",
    ])
    expect(opposing.zones.cemetery.cards.at(-1)?.object).toBe("a:maimu")
    expect(stats(own.last(), "a:meimu")).toEqual({ attack: 3, life: 1 })
    expect(stats(own.last(), "a:archer")).toEqual({ attack: 2, life: 2 })
    expect(valid()).toBe(true)
  })

  it.each([
    ["reject", 3],
    ["reject", 0],
  ] as const)(
    "takes no answer with nobody to deal to where the question says %s (total %i)",
    async (empty, total) => {
      const seats = scriptedTable(
        reshaped("distribute", "distribute", (shape) => ({
          ...shape,
          recipients: [],
          total,
          empty,
        })),
      )
      const own = follow(seats.p1)
      const asked = need(distribution(own.question(), "p1"))
      expect(await send(seats.p1, distributeAnswer(asked, new Map()))).toMatchObject(rejected)
      expect(seqs(own.messages)).toEqual([0])
    },
  )

  it("takes the empty answer with nobody to deal to where the question allows it", async () => {
    const seats = scriptedTable(
      reshaped("distribute", "distribute", (shape) => ({ ...shape, recipients: [] })),
    )
    const own = follow(seats.p1)
    const asked = need(distribution(own.question(), "p1"))
    expect(asked).toMatchObject({ total: 3, empty: "allow_without_total" })
    expect(await send(seats.p1, distributeAnswer(asked, new Map()))).toMatchObject(applied)
    expect(own.question()?.shape.kind).toBe("action_window")
  })

  it("takes no number the contract cannot carry, even in a range without an upper end", async () => {
    const seats = scriptedTable(
      reshaped("integer", "integer", (shape) => ({ ...shape, bounds: { min: 0, max: null } })),
    )
    const own = follow(seats.p1)
    const asked = need(integer(own.question(), "p1"))
    for (const value of [Number.MAX_SAFE_INTEGER + 1, 1.5])
      expect(await send(seats.p1, integerAnswer(asked, value))).toMatchObject(rejected)
    expect(await send(seats.p1, integerAnswer(asked, 3))).toMatchObject(applied)
  })

  it("takes a number within the question's range", async () => {
    const { seats, own, valid } = asking("integer")
    const asked = need(integer(own.question(), "p1"))
    const before = side(own.last())
    for (const value of [0, 4])
      expect(await send(seats.p1, integerAnswer(asked, value))).toMatchObject(rejected)
    expect(await send(seats.p1, integerAnswer(asked, 2))).toMatchObject(applied)
    expect(side(own.last())).toEqual({ ...before, hand: before.hand + 2, deck: before.deck - 2 })
    expect(valid()).toBe(true)
  })

  it("keeps a roll, or rolls again and has to keep what comes", async () => {
    const kept = asking("dice")
    expect(
      await send(kept.seats.p1, diceAnswer(need(dice(kept.own.question(), "p1")), "keep")),
    ).toMatchObject(applied)
    expect(kept.own.question()?.shape.kind).toBe("action_window")

    const { seats, own, other, valid } = asking("dice")
    const first = need(dice(own.question(), "p1"))
    expect(first).toMatchObject({ value: 3, remaining: 1 })
    expect(await send(seats.p1, diceAnswer(first, "reroll"))).toMatchObject(applied)
    // Nothing on the table moved: the other seat is sent nothing for the roll.
    expect(seqs(other.messages)).toEqual([0])
    const again = need(dice(own.question(), "p1"))
    expect(again).toMatchObject({ value: 5, remaining: 0, allowed: ["keep"] })
    expect(again.question).not.toBe(first.question)
    // No reroll is left, and the first roll's question is over.
    expect(await send(seats.p1, diceAnswer(again, "reroll"))).toMatchObject(rejected)
    expect(await send(seats.p1, diceAnswer(first, "keep"))).toMatchObject({ result: "rejected" })
    expect(await send(seats.p1, diceAnswer(again, "keep"))).toMatchObject(applied)
    expect(own.question()?.shape.kind).toBe("action_window")
    expect(valid()).toBe(true)
  })

  it.each([
    [false, { pp: 2, ep: 2 }],
    [true, { pp: 3, ep: 1 }],
  ] as const)(
    "evolves a follower: the payment (evolve point: %s), then what its evolved side does",
    async (useEp, points) => {
      const { seats, own, other, picking, valid } = asking("evolve")
      // The main-phase question offers the evolution of one follower.
      const acts = own.acts()
      expect([...acts.evolve.keys()]).toEqual(["a:priest"])
      const [evolution] = acts.evolve.get("a:priest") ?? []
      expect(await send(seats.p1, act(acts, need(evolution)))).toMatchObject(applied)
      const asked = need(evolutionPayment(own.question(), "p1"))
      expect(asked.payments).toEqual([
        { pp: 1, use_ep: false, super_evolution: false },
        { pp: 0, use_ep: true, super_evolution: false },
      ])
      // A payment the question does not list, though the seat could afford it.
      expect(
        await send(
          seats.p1,
          evolutionPaymentAnswer(asked, { pp: 1, use_ep: false, super_evolution: true }),
        ),
      ).toMatchObject(rejected)
      const payment = need(asked.payments.find((each) => each.use_ep === useEp))
      expect(await send(seats.p1, evolutionPaymentAnswer(asked, payment))).toMatchObject(applied)
      // Paid for and evolved: both seats see the evolve card on its follower.
      expect(side(own.last())).toMatchObject(points)
      expect(own.last().observation.objects["a:priest"]?.state).toMatchObject({
        evolution: "evolved",
        stats: { attack: 3, life: 3 },
      })
      expect(other.last().observation.players[0].zones.evolve_deck.count).toBe(9)
      // Its evolved side deals two damage to one of the other seat's followers.
      expect(need(selection(own.question(), "p1")).selection.bounds).toEqual({ min: 1, max: 1 })
      expect(await send(seats.p1, picking("own:unifier"))).toMatchObject(rejected)
      expect(await send(seats.p1, picking("foe:archer"))).toMatchObject(applied)
      const [, opposing] = own.last().observation.players
      expect(opposing.zones.field.cards.map((member) => member.object)).toEqual([
        "a:maimu",
        "a:meimu",
      ])
      expect(opposing.zones.cemetery.cards.at(-1)?.object).toBe("a:archer")
      expect(side(own.last())).toMatchObject(points)
      expect(own.question()?.shape.kind).toBe("action_window")
      expect(valid()).toBe(true)
    },
  )

  it("pays with the resources picked, an evolve point for a play point where that is allowed", async () => {
    const { seats, own, valid } = asking("resource")
    const asked = need(resourcePayment(own.question(), "p1"))
    const picks = (...ids: string[]) =>
      ids.reduce((made, id) => toggle(asked.materials, made, id), NO_PICKS)
    const before = side(own.last())
    // A resource the question does not list, and one named twice.
    for (const selected of [["foe:maimu"], ["own:priest", "own:priest"]])
      expect(
        await send(seats.p1, resourcePaymentAnswer(asked, { ...NO_PICKS, selected }, false)),
      ).toMatchObject(rejected)
    expect(
      await send(seats.p1, resourcePaymentAnswer(asked, picks("own:priest"), true)),
    ).toMatchObject(applied)
    expect(side(own.last())).toEqual({ ...before, ep: before.ep - 1 })
    expect(own.last().observation.objects["a:priest"]?.state?.posture).toBe("acted")
    expect(own.last().observation.objects["a:unifier"]?.state?.posture).toBe("standing")
    expect(valid()).toBe(true)
  })

  it("refuses an evolve point in place of a play point where the question does not allow it", async () => {
    const strict = reshaped("resource", "resource_payment", (shape) => ({
      ...shape,
      ep_allowed: false,
    }))
    const seats = scriptedTable(strict)
    const own = follow(seats.p1)
    const asked = need(resourcePayment(own.question(), "p1"))
    expect(asked.epAllowed).toBe(false)
    expect(await send(seats.p1, resourcePaymentAnswer(asked, NO_PICKS, true))).toMatchObject(
      rejected,
    )
    expect(await send(seats.p1, resourcePaymentAnswer(asked, NO_PICKS, false))).toMatchObject(
      applied,
    )
  })

  it("accepts a drive trigger for one follower the question lists, or declines it", async () => {
    const declined = asking("drive")
    const offered = need(driveTrigger(declined.own.question(), "p1"))
    expect(await send(declined.seats.p1, driveTriggerAnswer(offered, false, null))).toMatchObject(
      applied,
    )
    expect(stats(declined.own.last(), "a:priest")).toEqual({ attack: 2, life: 2 })

    const { seats, own, valid } = asking("drive")
    const asked = need(driveTrigger(own.question(), "p1"))
    // Accepted without the follower it has to name, and for one the question does not list.
    expect(await send(seats.p1, driveTriggerAnswer(asked, true, null))).toMatchObject(rejected)
    expect(await send(seats.p1, driveTriggerAnswer(asked, true, "foe:maimu"))).toMatchObject(
      rejected,
    )
    // Declined, it names nobody.
    expect(
      await send(seats.p1, {
        kind: "answer",
        question_id: asked.question,
        answer: { kind: "drive_trigger", accept: false, target: "own:priest" },
      }),
    ).toMatchObject(rejected)
    expect(await send(seats.p1, driveTriggerAnswer(asked, true, "own:priest"))).toMatchObject(
      applied,
    )
    expect(stats(own.last(), "a:priest")).toEqual({ attack: 3, life: 2 })
    expect(stats(own.last(), "a:unifier")).toEqual({ attack: 3, life: 4 })
    expect(valid()).toBe(true)
  })
})
