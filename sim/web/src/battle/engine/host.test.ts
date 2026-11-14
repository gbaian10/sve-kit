// @vitest-environment node
import { describe, expect, it } from "vitest"

import { BattleHost, type EngineSession, type SeatPort } from "./host"
import type {
  Delivery,
  InitMessage,
  MatchOptions,
  Seat,
  SeatMessage,
  SeatRequest,
} from "./protocol"

class FakePort implements SeatPort {
  readonly received: SeatMessage[] = []
  onmessage: ((event: MessageEvent<SeatRequest>) => void) | null = null
  postMessage(message: SeatMessage): void {
    this.received.push(message)
  }
  send(request: SeatRequest): void {
    this.onmessage?.({ data: request } as MessageEvent<SeatRequest>)
  }
}

const update = (seat: Seat, seq: number): Delivery => ({
  seat,
  message: {
    kind: "update",
    format: 0,
    viewer: seat,
    update_seq: seq,
    baseline: false,
    input: { kind: "waiting", actor: seat === "p1" ? "p2" : "p1" },
  },
})

/** Records every call and checks that no call starts while another is running. */
class FakeSession implements EngineSession {
  readonly calls: string[] = []
  #running = false
  #seq = 0
  #run(label: string): string {
    expect(this.#running).toBe(false)
    this.#running = true
    this.calls.push(label)
    this.#seq += 1
    const out = JSON.stringify([update("p1", this.#seq), update("p2", this.#seq)])
    this.#running = false
    return out
  }
  start(): string {
    return this.#run("start")
  }
  submit(seat: string, message: string): string {
    return this.#run(`submit ${seat} ${message}`)
  }
  baseline(seat: string): string {
    return this.#run(`baseline ${seat}`)
  }
}

const options: MatchOptions = {
  decks: [
    { construction: { kind: "class", value: "c" }, main: ["a"], evolve: [] },
    { construction: { kind: "class", value: "c" }, main: ["b"], evolve: [] },
  ],
  control: ["player", "player"],
  max_submissions: 10,
}

const inputs = { snapshot: "s", keywords: "k", selection: "{}", rules: "{}" }

function setUp(
  create: (setup: string) => Promise<EngineSession>,
  ports: Partial<Record<Seat, FakePort>>,
  matchOptions: MatchOptions = options,
) {
  const host = new BattleHost(
    (_inputs, setup) => create(setup),
    (bytes) => bytes.fill(7),
  )
  host.init({
    kind: "init",
    inputs,
    options: matchOptions,
    ports: ports as unknown as InitMessage["ports"],
  })
  return host
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 0))

describe("BattleHost", () => {
  it("queues requests sent during loading and runs every command in arrival order", async () => {
    const p1 = new FakePort()
    const p2 = new FakePort()
    let session: FakeSession | undefined
    let release: () => void = () => undefined
    const loaded = new Promise<void>((resolve) => {
      release = resolve
    })
    setUp(
      async () => {
        await loaded
        session = new FakeSession()
        return session
      },
      { p1, p2 },
    )
    const submit = { request_id: "r1", command: { kind: "concede" as const } }
    p2.send({ kind: "submit", submit })
    p1.send({ kind: "baseline", request_id: "b1" })
    await settle()
    expect(p1.received).toEqual([])
    release()
    await settle()
    expect(session?.calls).toEqual(["start", `submit p2 ${JSON.stringify(submit)}`, "baseline p1"])
    // Each seat only receives its own deliveries; the baseline reply is announced first.
    expect(p1.received.map((message) => message.kind)).toEqual([
      "update",
      "update",
      "baseline_start",
      "update",
    ])
    expect(p1.received[2]).toEqual({ kind: "baseline_start", request_id: "b1" })
    expect(
      p1.received.every((message) => message.kind !== "update" || message.viewer === "p1"),
    ).toBe(true)
    expect(
      p2.received.every((message) => message.kind === "update" && message.viewer === "p2"),
    ).toBe(true)
  })

  it("draws the seed and first chooser inside the Worker unless fixed", async () => {
    const setups: string[] = []
    const create = (setup: string) => {
      setups.push(setup)
      return Promise.resolve(new FakeSession())
    }
    setUp(create, { p1: new FakePort() })
    setUp(create, { p1: new FakePort() }, { ...options, seed: "fixed", first_chooser: "p2" })
    await settle()
    const [drawn, fixed] = setups.map((setup) => JSON.parse(setup) as Record<string, unknown>)
    expect(drawn).toMatchObject({
      seed: "07".repeat(16),
      first_chooser: "p2",
      control: options.control,
    })
    expect(fixed).toMatchObject({ seed: "fixed", first_chooser: "p2", max_submissions: 10 })
  })

  it("routes nothing to a policy seat, which has no port", async () => {
    const p1 = new FakePort()
    setUp(
      () => Promise.resolve(new FakeSession()),
      { p1 },
      {
        ...options,
        control: ["player", "policy"],
      },
    )
    await settle()
    expect(p1.received).toHaveLength(1)
  })

  it("reports load failures by code only and ignores later requests", async () => {
    for (const [message, code] of [
      ["selection", "selection"],
      ["format", "format"],
      ["match", "match"],
      ["RuntimeError: unreachable at secret.rs:1", "engine"],
    ] as const) {
      const p1 = new FakePort()
      setUp(() => Promise.reject(new Error(message)), { p1 })
      p1.send({ kind: "baseline", request_id: "b1" })
      await settle()
      expect(p1.received).toEqual([{ kind: "load_error", code }])
    }
  })

  it("stops after an engine trap and tells every seat once", async () => {
    const p1 = new FakePort()
    const p2 = new FakePort()
    const trapping: EngineSession = {
      start: () => "[]",
      submit: () => {
        throw new Error("unreachable")
      },
      baseline: () => "[]",
    }
    setUp(() => Promise.resolve(trapping), { p1, p2 })
    p1.send({ kind: "submit", submit: { request_id: "r", command: { kind: "concede" } } })
    p1.send({ kind: "baseline", request_id: "b1" })
    await settle()
    const stopped = { kind: "host_error", format: 1, reason: "projection" }
    expect(p1.received).toEqual([stopped])
    expect(p2.received).toEqual([stopped])
  })
})
