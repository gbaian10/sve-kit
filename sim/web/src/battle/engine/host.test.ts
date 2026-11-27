// @vitest-environment node
import { readFileSync } from "node:fs"
import path from "node:path"

import { describe, expect, it } from "vitest"

import { type Notice, type Submit, validateHostMessage } from "../contract"
import { BattleHost, type EngineSession, type SeatPort } from "./host"
import type {
  BattleUpdate,
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

/** The contract's example of a seat waiting for its opponent. */
const waiting = JSON.parse(
  readFileSync(
    path.resolve(
      import.meta.dirname,
      "../../../../battle-contract/fixtures/host/update-waiting.json",
    ),
    "utf8",
  ),
) as BattleUpdate

const update = (seat: Seat, seq: number): Delivery => ({
  seat,
  message: { ...waiting, viewer: seat, update_seq: seq },
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

function setUp(
  create: (setup: string) => Promise<EngineSession>,
  ports: Partial<Record<Seat, FakePort>>,
  matchOptions: MatchOptions = options,
  validate?: ((message: unknown) => boolean) | Promise<(message: unknown) => boolean>,
) {
  const host = new BattleHost(
    (setup) => create(setup),
    (bytes) => bytes.fill(7),
    validate,
  )
  host.init({
    kind: "init",
    options: matchOptions,
    ports: ports as unknown as InitMessage["ports"],
  })
  return host
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 0))

describe("BattleHost", () => {
  it("uses the latest preference for unsent answers and delivers Look before continuing", async () => {
    const p1 = new FakePort()
    const p2 = new FakePort()
    const commands: { seat: string; submit: Submit }[] = []
    const look: Notice = {
      id: "only-p1",
      kind: "look",
      source: null,
      cards: [{ card: { printing_id: "secret", face: 0 }, owner: "p1", position: 0 }],
    }
    const pass = { kind: "quick", option: null } as const
    const ask = (id: string, seq: number, publicProof: boolean, notices: Notice[]): Delivery => ({
      seat: "p1",
      message: {
        ...waiting,
        viewer: "p1",
        observation: { ...waiting.observation, viewer: "p1" },
        update_seq: seq,
        notices,
        input: {
          kind: "question",
          question: {
            id,
            actor: "p1",
            purpose: "quick_window",
            use: "quick",
            processing: {
              stage: "quick",
              controller: null,
              actor: "p1",
              source: null,
              subject: null,
              rule: null,
              procedure: null,
              declared: [],
              payments: [],
            },
            shape: { kind: "quick", window: "end_phase", options: [] },
            can_withdraw: false,
            auto_answer: { kind: "unique", answer: pass, public: publicProof },
            assistance: null,
            rules: null,
          },
        },
      },
    })
    const first = ask("q-first", 1, true, [look])
    const following = ask("q-second", 2, false, [])
    const session: EngineSession = {
      start: () => JSON.stringify([first, update("p2", 1)]),
      submit(seat, message) {
        const submit = JSON.parse(message) as Submit
        commands.push({ seat, submit })
        return JSON.stringify([
          {
            seat,
            message: {
              kind: "receipt",
              request_id: submit.request_id,
              result: "applied",
              rejection: null,
            },
          },
          following,
        ])
      },
      baseline: () => JSON.stringify([]),
    }
    setUp(() => Promise.resolve(session), { p1, p2 }, options, validateHostMessage)
    p2.send({ kind: "auto_answer", enabled: true })
    await settle()
    expect(commands).toEqual([])
    p1.send({ kind: "auto_answer", enabled: true })
    p1.send({ kind: "auto_answer", enabled: false })
    await settle()
    await settle()
    expect(commands).toEqual([])
    expect(p1.received).toEqual([first.message])
    p1.send({ kind: "auto_answer", enabled: true })
    await settle()
    await settle()
    expect(commands).toEqual([
      {
        seat: "p1",
        submit: {
          kind: "submit",
          request_id: "local-auto-q-first",
          command: { kind: "answer", question_id: "q-first", answer: pass },
        },
      },
    ])
    expect(p1.received).toEqual([
      first.message,
      { kind: "receipt", request_id: "local-auto-q-first", result: "applied", rejection: null },
      following.message,
    ])
    expect(p2.received).toEqual([update("p2", 1).message])
  })

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
    const submit = {
      kind: "submit" as const,
      request_id: "r1",
      command: { kind: "concede" as const },
    }
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
      ["identity", "identity"],
      ["references", "references"],
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

  it("sends each deck failure stage only to its submitting seat", async () => {
    for (const detail of [
      "deck_format",
      "deck_printing",
      "selection",
      "references",
      "deck_admission",
    ]) {
      for (const submitter of ["p1", "p2"] as const) {
        const ports = { p1: new FakePort(), p2: new FakePort() }
        const other = submitter === "p1" ? "p2" : "p1"
        const codes = submitter === "p1" ? [detail, "match"] : ["match", detail]
        setUp(() => Promise.reject(new Error(JSON.stringify(codes))), ports)
        ports[other].send({ kind: "baseline", request_id: "after-failed-open" })
        await settle()
        expect(ports[submitter].received).toEqual([{ kind: "load_error", code: detail }])
        expect(ports[other].received).toEqual([{ kind: "load_error", code: "match" }])
      }
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
    p1.send({
      kind: "submit",
      submit: { kind: "submit", request_id: "r", command: { kind: "concede" } },
    })
    p1.send({ kind: "baseline", request_id: "b1" })
    await settle()
    const stopped = { kind: "host_error", format: 1, reason: "projection" }
    expect(p1.received).toEqual([stopped])
    expect(p2.received).toEqual([stopped])
  })

  it("checks every message, updates included, against the generated contract validator", async () => {
    const receipt = (result: string) =>
      JSON.stringify([
        {
          seat: "p1",
          message: { kind: "receipt", request_id: "r", result, rejection: "invalid_submission" },
        },
      ])
    const run = async (submitted: string) => {
      const p1 = new FakePort()
      const session: EngineSession = {
        start: () => "[]",
        submit: () => submitted,
        baseline: () => "[]",
      }
      setUp(() => Promise.resolve(session), { p1 }, options, validateHostMessage)
      p1.send({
        kind: "submit",
        submit: { kind: "submit", request_id: "r", command: { kind: "concede" } },
      })
      p1.send({ kind: "baseline", request_id: "b1" })
      await settle()
      return p1.received
    }
    const stopped = { kind: "host_error", format: 1, reason: "projection" }
    expect(validateHostMessage(stopped)).toBe(true)
    expect(await run(receipt("rejected"))).toEqual([
      { kind: "receipt", request_id: "r", result: "rejected", rejection: "invalid_submission" },
      { kind: "baseline_start", request_id: "b1" },
    ])
    // A result outside the contract is never delivered; interaction stops instead.
    expect(await run(receipt("maybe"))).toEqual([stopped])
    const valid = update("p1", 9)
    expect(await run(JSON.stringify([valid]))).toEqual([
      valid.message,
      { kind: "baseline_start", request_id: "b1" },
    ])
    const advised = { ...valid, message: { ...waiting, advice: ["hint"] } }
    expect(await run(JSON.stringify([advised]))).toEqual([stopped])
  })

  it("queues commands while a lazily imported validator loads, then still validates", async () => {
    const p1 = new FakePort()
    let provide: (validate: (message: unknown) => boolean) => void = () => undefined
    const validator = new Promise<(message: unknown) => boolean>((resolve) => {
      provide = resolve
    })
    const session: EngineSession = {
      start: () => "[]",
      submit: () =>
        JSON.stringify([
          { seat: "p1", message: { kind: "receipt", request_id: "r", result: "maybe" } },
        ]),
      baseline: () => "[]",
    }
    setUp(() => Promise.resolve(session), { p1 }, options, validator)
    p1.send({
      kind: "submit",
      submit: { kind: "submit", request_id: "r", command: { kind: "concede" } },
    })
    await settle()
    expect(p1.received).toEqual([])
    provide(validateHostMessage)
    await settle()
    expect(p1.received).toEqual([{ kind: "host_error", format: 1, reason: "projection" }])
  })
})
