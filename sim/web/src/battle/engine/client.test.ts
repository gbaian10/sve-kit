// @vitest-environment node
import { afterEach, describe, expect, it } from "vitest"

import {
  type BattleWorker,
  ChannelClosedError,
  ChannelStoppedError,
  CommandOutstandingError,
  HotSeat,
  SeatChannel,
  startLocalBattle,
} from "./client"
import type {
  InitMessage,
  MatchOptions,
  Receipt,
  Seat,
  SeatMessage,
  SeatRequest,
  Update,
} from "./protocol"

const opened: MessagePort[] = []
afterEach(() => {
  for (const port of opened.splice(0)) port.close()
})

/** A channel plus the Worker end of its port. */
function pair(seat: Seat = "p1") {
  const { port1, port2 } = new MessageChannel()
  opened.push(port1, port2)
  const requests: SeatRequest[] = []
  port2.onmessage = (event: MessageEvent<SeatRequest>) => {
    requests.push(event.data)
  }
  return { channel: new SeatChannel(seat, port1), worker: port2, requests }
}

const tick = () => new Promise((resolve) => setTimeout(resolve, 10))

const receipt = (id: string): Receipt => ({
  kind: "receipt",
  request_id: id,
  result: "applied",
  rejection: null,
})

const update = (seat: Seat, seq: number, baseline = false): Update => ({
  kind: "update",
  format: 0,
  viewer: seat,
  update_seq: seq,
  baseline,
  input: {
    kind: "question",
    id: `${seat}-q${String(seq)}`,
    purpose: "main_action",
    shape: "action_window",
  },
})

const concede = { kind: "concede" } as const

describe("SeatChannel", () => {
  it("allows one command in flight and pairs its receipt by request id", async () => {
    const { channel, worker, requests } = pair()
    const first = channel.submit(concede)
    await expect(channel.submit(concede)).rejects.toBeInstanceOf(CommandOutstandingError)
    await tick()
    expect(requests).toEqual([
      { kind: "submit", submit: { request_id: "p1-c1", command: concede } },
    ])
    expect(channel.busy).toBe(true)
    worker.postMessage(receipt("someone-else"))
    worker.postMessage(receipt("p1-c1"))
    await expect(first).resolves.toEqual(receipt("p1-c1"))
    expect(channel.busy).toBe(false)
    void channel.submit(concede)
    await tick()
    expect(requests.at(-1)).toEqual({
      kind: "submit",
      submit: { request_id: "p1-c2", command: concede },
    })
  })

  it("passes other messages to listeners in order and fails the command on a host error", async () => {
    const { channel, worker } = pair()
    const seen: SeatMessage[] = []
    channel.listen((message) => seen.push(message))
    const pending = channel.submit(concede)
    worker.postMessage(update("p1", 1))
    worker.postMessage({ kind: "host_error", format: 1, reason: "budget" })
    await expect(pending).rejects.toBeInstanceOf(ChannelStoppedError)
    expect(seen.map((message) => message.kind)).toEqual(["update", "host_error"])
  })

  it("ignores everything after close and refuses new commands", async () => {
    const { channel, worker, requests } = pair()
    const seen: SeatMessage[] = []
    channel.listen((message) => seen.push(message))
    const pending = channel.submit(concede)
    channel.close()
    await expect(pending).rejects.toBeInstanceOf(ChannelClosedError)
    worker.postMessage(update("p1", 9))
    worker.postMessage(receipt("p1-c1"))
    channel.baseline()
    await tick()
    expect(seen).toEqual([])
    await expect(channel.submit(concede)).rejects.toBeInstanceOf(ChannelClosedError)
    // Only the command sent before close reached the Worker; nothing after it.
    expect(requests).toEqual([
      { kind: "submit", submit: { request_id: "p1-c1", command: concede } },
    ])
  })
})

describe("HotSeat", () => {
  function hotSeat(viewer: Seat = "p1") {
    const p1 = pair("p1")
    const p2 = pair("p2")
    const shown: [Seat, number | undefined][] = []
    const hot = new HotSeat({ p1: p1.channel, p2: p2.channel }, viewer, (seat, latest) => {
      shown.push([seat, latest?.update_seq])
    })
    return { hot, p1, p2, shown }
  }

  /** Reply to a baseline request the way the Worker does: marker first, then the update. */
  function reply(worker: MessagePort, request: SeatRequest | undefined, latest: Update) {
    if (request?.kind !== "baseline") throw new Error("expected a baseline request")
    worker.postMessage({ kind: "baseline_start", request_id: request.request_id })
    worker.postMessage(latest)
  }

  it("shows only the viewer and reloads that seat after a switch", async () => {
    const { hot, p1, p2, shown } = hotSeat()
    p1.worker.postMessage(update("p1", 1, true))
    p2.worker.postMessage(update("p2", 1, true))
    await tick()
    expect(shown).toEqual([["p1", 1]])

    hot.switchTo("p2")
    expect(hot.latest).toBeUndefined()
    p1.worker.postMessage(update("p1", 2))
    await tick()
    expect(p2.requests).toEqual([{ kind: "baseline", request_id: "p2-b1" }])
    reply(p2.worker, p2.requests[0], update("p2", 1, true))
    p2.worker.postMessage(update("p2", 2))
    await tick()
    expect(hot.viewer).toBe("p2")
    expect(shown).toEqual([
      ["p1", 1],
      ["p2", undefined],
      ["p2", 1],
      ["p2", 2],
    ])

    void hot.submit(concede)
    await tick()
    expect(p2.requests.at(-1)).toMatchObject({ kind: "submit" })
    expect(p1.requests).toEqual([])
  })

  it("does not take an older queued baseline as the reply to this switch", async () => {
    const { hot, p2, shown } = hotSeat()
    // The start baseline and a later update for p2 are still on the port when the user switches.
    p2.worker.postMessage(update("p2", 1, true))
    p2.worker.postMessage(update("p2", 2))
    hot.switchTo("p2")
    await tick()
    expect(hot.latest).toBeUndefined()
    expect(shown).toEqual([["p2", undefined]])
    reply(p2.worker, p2.requests[0], update("p2", 2, true))
    await tick()
    expect(hot.latest).toEqual(update("p2", 2, true))
  })

  it("ignores replies to earlier switches after switching back and forth", async () => {
    const { hot, p1, p2 } = hotSeat()
    hot.switchTo("p2")
    hot.switchTo("p1")
    hot.switchTo("p2")
    await tick()
    expect(p2.requests).toEqual([
      { kind: "baseline", request_id: "p2-b1" },
      { kind: "baseline", request_id: "p2-b2" },
    ])
    expect(p1.requests).toEqual([{ kind: "baseline", request_id: "p1-b1" }])
    reply(p2.worker, p2.requests[0], update("p2", 1, true))
    p2.worker.postMessage(update("p2", 2))
    await tick()
    expect(hot.latest).toBeUndefined()
    reply(p2.worker, p2.requests[1], update("p2", 2, true))
    await tick()
    expect(hot.latest).toEqual(update("p2", 2, true))
  })

  for (const stop of [
    { kind: "host_error", format: 1, reason: "projection" },
    { kind: "load_error", code: "engine" },
  ] as const) {
    for (const inFlight of [false, true]) {
      it(`stops for good on ${stop.kind} (command in flight: ${String(inFlight)})`, async () => {
        const { hot, p1, p2, shown } = hotSeat()
        p1.worker.postMessage(update("p1", 1, true))
        await tick()
        expect(hot.latest?.input.kind).toBe("question")
        // Settle into a value right away so the rejection is observed before the assertion.
        const pending = inFlight
          ? hot.submit(concede).then(
              () => "receipt",
              (error: unknown) => error,
            )
          : undefined
        await tick()
        const sent = p1.requests.length
        // The Worker broadcasts the stop to every seat.
        p1.worker.postMessage(stop)
        p2.worker.postMessage(stop)
        await tick()
        if (pending !== undefined) expect(await pending).toBeInstanceOf(ChannelStoppedError)
        expect(hot.stopped).toEqual(stop)
        expect(hot.latest).toBeUndefined()
        expect(shown.at(-1)).toEqual(["p1", undefined])
        await expect(hot.submit(concede)).rejects.toBeInstanceOf(ChannelStoppedError)
        await expect(p1.channel.submit(concede)).rejects.toBeInstanceOf(ChannelStoppedError)
        expect(p1.channel.baseline()).toBeUndefined()
        hot.switchTo("p2")
        // A late update cannot revive interaction.
        p1.worker.postMessage(update("p1", 2))
        p2.worker.postMessage(update("p2", 2, true))
        await tick()
        expect(hot.latest).toBeUndefined()
        expect(p1.requests).toHaveLength(sent)
        expect(p2.requests).toEqual([])
        expect(p1.channel.stopped).toEqual(stop)
        expect(p1.channel.busy).toBe(false)
      })
    }
  }
})

describe("startLocalBattle", () => {
  it("transfers one port per player seat and tears everything down on terminate", async () => {
    let init: InitMessage | undefined
    let transferred: Transferable[] = []
    let terminated = false
    const worker: BattleWorker = {
      postMessage(message, transfer) {
        init = message
        transferred = transfer
      },
      terminate() {
        terminated = true
      },
    }
    const options: MatchOptions = {
      decks: [
        { construction: { kind: "class", value: "c" }, main: [], evolve: [] },
        { construction: { kind: "class", value: "c" }, main: [], evolve: [] },
      ],
      control: ["policy", "player"],
      max_submissions: 1,
    }
    const inputs = { snapshot: "", keywords: "", selection: "", rules: "" }
    const battle = startLocalBattle(inputs, options, () => worker)
    expect(Object.keys(battle.channels)).toEqual(["p2"])
    expect(Object.keys(init?.ports ?? {})).toEqual(["p2"])
    expect(transferred).toHaveLength(1)
    const port = init?.ports.p2
    if (port === undefined) throw new Error("p2 port missing")
    opened.push(port)
    const seen: SeatMessage[] = []
    battle.channels.p2?.listen((message) => seen.push(message))
    battle.terminate()
    expect(terminated).toBe(true)
    port.postMessage(update("p2", 1, true))
    await tick()
    expect(seen).toEqual([])
  })
})
