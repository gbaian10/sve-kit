// @vitest-environment node
import { afterEach, describe, expect, it } from "vitest"

import {
  type BattleWorker,
  ChannelClosedError,
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
    worker.postMessage({ kind: "host_error", format: 0, reason: "budget" })
    await expect(pending).rejects.toBeInstanceOf(ChannelClosedError)
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
  it("shows only the viewer and reloads a baseline after a switch", async () => {
    const p1 = pair("p1")
    const p2 = pair("p2")
    const shown: [Seat, Update | undefined][] = []
    const hot = new HotSeat({ p1: p1.channel, p2: p2.channel }, "p1", (viewer, latest) => {
      shown.push([viewer, latest])
    })
    p1.worker.postMessage(update("p1", 1, true))
    p2.worker.postMessage(update("p2", 1, true))
    await tick()
    expect(shown).toEqual([["p1", update("p1", 1, true)]])

    hot.switchTo("p2")
    expect(hot.latest).toBeUndefined()
    // A late non-baseline update queued before the switch is older than the baseline.
    p2.worker.postMessage(update("p2", 2))
    p1.worker.postMessage(update("p1", 2))
    await tick()
    expect(p2.requests).toEqual([{ kind: "baseline" }])
    expect(shown.at(-1)).toEqual(["p2", undefined])
    p2.worker.postMessage(update("p2", 2, true))
    p2.worker.postMessage(update("p2", 3))
    await tick()
    expect(hot.viewer).toBe("p2")
    expect(hot.latest).toEqual(update("p2", 3))

    void hot.submit(concede)
    await tick()
    expect(p2.requests.at(-1)).toMatchObject({ kind: "submit" })
    expect(p1.requests).toEqual([])
  })
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
