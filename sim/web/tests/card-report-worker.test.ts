// @vitest-environment node
import { generateKeyPairSync, verify } from "node:crypto"

import { afterEach, expect, it, vi } from "vitest"

import worker from "../src/cloudflare/worker"

const siteverify = "https://challenges.cloudflare.com/turnstile/v0/siteverify"
const githubInstallation = "https://api.github.com/app/installations/456/access_tokens"
const githubIssues = "https://api.github.com/repos/gbaian10/sve-kit/issues"
const report = {
  cardNo: "BP03-LDⓈ01EN",
  printingIntId: null,
  edition: "en",
  uiLanguage: "zh-TW",
  dataVersion: "preview-20261121T010000Z-0001",
  type: "translation",
  description: "Example wording",
  contact: "",
  token: "XXXX.DUMMY.TOKEN.XXXX",
}
const { privateKey, publicKey } = generateKeyPairSync("rsa", { modulusLength: 2048 })

function fixture(configured = true) {
  const limit = vi
    .fn<(input: { key: string }) => Promise<{ success: boolean }>>()
    .mockResolvedValue({ success: true })
  const upstream = vi.fn<typeof fetch>()
  vi.stubGlobal("fetch", upstream)
  const log = vi.spyOn(console, "error").mockImplementation(() => undefined)
  const env: Parameters<typeof worker.fetch>[1] = {
    PREVIEW_BUCKET: {
      get: () => Promise.reject(new Error("Unexpected bucket access")),
      head: () => Promise.reject(new Error("Unexpected bucket access")),
    },
    ASSETS: { fetch: () => Promise.reject(new Error("Unexpected asset request")) },
    ...(configured ? { TURNSTILE_SECRET: "1x0000000000000000000000000000000AA" } : {}),
    REPORT_GITHUB_APP_ID: "123",
    REPORT_GITHUB_INSTALLATION_ID: "456",
    REPORT_GITHUB_PRIVATE_KEY: privateKey.export({ type: "pkcs8", format: "pem" }),
    CARD_REPORT_LIMITER: { limit },
  }
  const send = (body: object = report) =>
    worker.fetch(
      new Request("https://dev.svekit.app/api/card-reports", {
        method: "POST",
        headers: { "Content-Type": "application/json", "CF-Connecting-IP": "192.0.2.1" },
        body: JSON.stringify(body),
      }),
      env,
      { waitUntil: () => undefined },
    )
  return { limit, log, send, upstream }
}

afterEach(() => {
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
})

it("rejects invalid fields before calling external services", async () => {
  const { log, send, upstream } = fixture()
  const invalidFields = [
    { cardNo: "../BP03-001" },
    { description: "Unexpected\u0000NUL" },
    { contact: "Unexpected\u202eoverride" },
  ]
  for (const fields of invalidFields) {
    const response = await send({ ...report, ...fields })
    expect(response.status).toBe(400)
    expect(await response.json()).toEqual({ error: "invalid-report" })
  }
  expect(upstream).not.toHaveBeenCalled()
  expect(log.mock.calls).toEqual(invalidFields.map(() => ["card-report: fields"]))
})

it("refuses a report with a rejected Turnstile token", async () => {
  const { send, upstream, limit, log } = fixture()
  upstream.mockResolvedValueOnce(Response.json({ success: false }))

  const response = await send()

  expect(response.status).toBe(403)
  expect(await response.json()).toEqual({ error: "verification-failed" })
  expect(upstream).toHaveBeenCalledExactlyOnceWith(siteverify, expect.anything())
  expect(limit).not.toHaveBeenCalled()
  expect(log).toHaveBeenCalledExactlyOnceWith("card-report: siteverify")
})

it("refuses a report over the rate limit without contacting GitHub", async () => {
  const { send, upstream, limit, log } = fixture()
  upstream.mockResolvedValueOnce(Response.json({ success: true }))
  limit.mockResolvedValueOnce({ success: false })

  const response = await send()

  expect(response.status).toBe(429)
  expect(await response.json()).toEqual({ error: "rate-limited" })
  expect(upstream).toHaveBeenCalledExactlyOnceWith(siteverify, expect.anything())
  expect(log).toHaveBeenCalledExactlyOnceWith("card-report: rate-limit")
})

it("reports missing secrets without calling external services", async () => {
  const { log, send, upstream } = fixture(false)

  const response = await send()

  expect(response.status).toBe(503)
  expect(await response.json()).toEqual({ error: "not-configured" })
  expect(upstream).not.toHaveBeenCalled()
  expect(log).toHaveBeenCalledExactlyOnceWith("card-report: configuration")
})

it("opens a labeled issue with validated metadata and literal user text", async () => {
  const { send, upstream, limit } = fixture()
  upstream.mockImplementation(async (url, init) => {
    const request = new Request(url, init)
    const payload: unknown = await request.json()
    switch (url) {
      case siteverify:
        expect(payload).toEqual({
          secret: "1x0000000000000000000000000000000AA",
          response: report.token,
        })
        return Response.json({ success: true })
      case githubInstallation: {
        const jwt = request.headers.get("Authorization")?.replace(/^Bearer /u, "") ?? ""
        const [header, claims, signature] = jwt.split(".")
        if (!header || !claims || !signature) throw new Error("Missing signed JWT")
        expect(
          verify(
            "RSA-SHA256",
            Buffer.from(`${header}.${claims}`),
            publicKey,
            Buffer.from(signature, "base64url"),
          ),
        ).toBe(true)
        expect(payload).toEqual({ repositories: ["sve-kit"], permissions: { issues: "write" } })
        return Response.json({ token: "test-installation-token" }, { status: 201 })
      }
      case githubIssues:
        expect(request.headers.get("Authorization")).toBe("Bearer test-installation-token")
        expect(payload).toMatchObject({
          title: "[EN] BP03-LDⓈ01EN · translation",
          labels: ["card-report"],
        })
        expect(payload).toHaveProperty(
          "body",
          expect.stringContaining(
            "\n    @\u200bexample\n    ![image](https://example.com/image.png)\n    <img\tsrc=x>",
          ),
        )
        expect(payload).toHaveProperty(
          "body",
          expect.stringContaining(
            "https://en.shadowverse-evolve.com/cards/?cardno=BP03-LD%E2%93%8801EN",
          ),
        )
        return Response.json({ number: 987 }, { status: 201 })
      default:
        throw new Error("Unexpected upstream request")
    }
  })

  const response = await send({
    ...report,
    description: "@example\r\n![image](https://example.com/image.png)\r<img\tsrc=x>",
    contact: "card@example.com",
  })

  expect(response.status).toBe(201)
  expect(await response.json()).toEqual({ issueNumber: 987 })
  expect(limit.mock.calls[0]?.[0].key).toMatch(/^[A-Za-z0-9_-]{43}$/u)
})
