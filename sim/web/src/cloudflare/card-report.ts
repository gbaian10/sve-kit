import {
  CARD_REPORT_REPOSITORY,
  CARD_REPORT_TYPES,
  type CardReportInput,
  REPORT_CONTACT_LIMIT,
  REPORT_DESCRIPTION_LIMIT,
  REPORT_TURNSTILE_ACTION,
} from "../domain/card-report"
import { officialCardUrl } from "../domain/cardRoute"
import { isUiLanguage } from "../i18n/languages"

export interface CardReportEnv {
  readonly TURNSTILE_SECRET?: string
  readonly CARD_REPORT_GITHUB_APP_ID?: string
  readonly CARD_REPORT_GITHUB_INSTALLATION_ID?: string
  readonly CARD_REPORT_GITHUB_PRIVATE_KEY?: string
  readonly CARD_REPORT_LIMITER?: { limit(input: { key: string }): Promise<{ success: boolean }> }
}

const SITEVERIFY = "https://challenges.cloudflare.com/turnstile/v0/siteverify"
const TEST_TURNSTILE_SECRET = "1x0000000000000000000000000000000AA"
const GITHUB = "https://api.github.com"
const MAX_BODY_BYTES = 32 * 1024
const CARD_NO = /^[A-Z0-9][A-Za-z0-9]*-(?:[A-ZⓈ]*[0-9]+[A-Za-z0-9Ⓢ]*|S?EP)$/u
const DATA_VERSION = /^(?:preview-)?[0-9]{8}T[0-9]{6}Z-[0-9]{4}$/u
const encoder = new TextEncoder()

function json(body: Record<string, string | number>, status: number): Response {
  return Response.json(body, {
    status,
    headers: {
      "Cache-Control": "no-store",
      ...(status === 405 ? { Allow: "POST" } : {}),
    },
  })
}

async function readReport(request: Request): Promise<CardReportInput | null> {
  const reader = request.body?.getReader()
  if (!reader) return null

  try {
    const buffer = new Uint8Array(MAX_BODY_BYTES)
    let size = 0
    for (;;) {
      const { value, done } = await reader.read()
      if (done) break
      if (size + value.byteLength > MAX_BODY_BYTES) {
        await reader.cancel()
        return null
      }
      buffer.set(value, size)
      size += value.byteLength
    }
    const value: unknown = JSON.parse(
      new TextDecoder("utf-8", { fatal: true }).decode(buffer.subarray(0, size)),
    )
    if (typeof value !== "object" || value === null || Array.isArray(value)) return null
    const {
      cardNo,
      printingIntId,
      edition,
      uiLanguage,
      dataVersion,
      type,
      description,
      contact,
      token,
    } = value as Record<string, unknown>
    const reportType = CARD_REPORT_TYPES.find((candidate) => candidate === type)
    if (
      (cardNo !== null &&
        (typeof cardNo !== "string" || cardNo.length > 32 || !CARD_NO.test(cardNo))) ||
      (cardNo === null
        ? typeof printingIntId !== "number" ||
          !Number.isInteger(printingIntId) ||
          printingIntId <= 0 ||
          printingIntId > 0xffffffff
        : printingIntId !== null) ||
      (edition !== "jp" && edition !== "en") ||
      !isUiLanguage(uiLanguage) ||
      typeof dataVersion !== "string" ||
      !DATA_VERSION.test(dataVersion) ||
      !reportType ||
      typeof description !== "string" ||
      description.length > REPORT_DESCRIPTION_LIMIT ||
      !description.trim() ||
      typeof contact !== "string" ||
      contact.length > REPORT_CONTACT_LIMIT ||
      typeof token !== "string" ||
      token.length > 2048 ||
      !token.trim()
    )
      return null
    return {
      cardNo,
      printingIntId: typeof printingIntId === "number" ? printingIntId : null,
      edition,
      uiLanguage,
      dataVersion,
      type: reportType,
      description: description.trim(),
      contact: contact.trim(),
      token,
    }
  } catch {
    return null
  }
}

async function post(
  url: string,
  body: Record<string, unknown>,
  headers?: Record<string, string>,
): Promise<unknown> {
  // eslint-disable-next-line no-restricted-globals -- Upstream credentials are only available in the Worker.
  const response = await fetch(url, {
    method: "POST",
    headers: { "Content-Type": "application/json", ...headers },
    body: JSON.stringify(body),
    redirect: "error",
    signal: AbortSignal.timeout(10_000),
  })
  if (!response.ok) throw new Error("Upstream request failed")
  return response.json()
}

function field(value: unknown, name: string): unknown {
  return typeof value === "object" && value !== null
    ? (value as Record<string, unknown>)[name]
    : undefined
}

function base64Url(binary: string): string {
  return btoa(binary).replaceAll("+", "-").replaceAll("/", "_").replace(/=+$/u, "")
}

async function signAppJwt(appId: string, privateKey: string): Promise<string> {
  const algorithm = { name: "RSASSA-PKCS1-v1_5", hash: "SHA-256" }
  const pem = privateKey.replace(/-----BEGIN PRIVATE KEY-----|-----END PRIVATE KEY-----|\s/gu, "")
  const key = await crypto.subtle.importKey(
    "pkcs8",
    Uint8Array.from(atob(pem), (char) => char.charCodeAt(0)),
    algorithm,
    false,
    ["sign"],
  )
  const now = Math.floor(Date.now() / 1000)
  const unsigned = [
    base64Url(JSON.stringify({ alg: "RS256", typ: "JWT" })),
    base64Url(JSON.stringify({ iat: now - 60, exp: now + 300, iss: appId })),
  ].join(".")
  const signature = await crypto.subtle.sign(algorithm, key, encoder.encode(unsigned))
  return `${unsigned}.${base64Url(String.fromCharCode(...new Uint8Array(signature)))}`
}

async function sourceKey(request: Request, secret: string): Promise<string> {
  const ip = request.headers.get("CF-Connecting-IP") ?? ""
  const key = await crypto.subtle.importKey(
    "raw",
    encoder.encode(secret),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  )
  const signed = await crypto.subtle.sign("HMAC", key, encoder.encode(`card-report:${ip}`))
  return base64Url(String.fromCharCode(...new Uint8Array(signed)))
}

function plainTextMarkdown(text: string): string {
  // Indent every physical line so user Markdown cannot escape the literal block.
  return text
    .replace(/(?<![\p{L}\p{N}._%+-])@/gu, "@\u200b")
    .split(/\r\n|[\r\n]/u)
    .map((line) => `    ${line}`)
    .join("\n")
}

async function createIssue(
  report: CardReportInput,
  jwt: string,
  installationId: string,
): Promise<number> {
  const githubHeaders = {
    Accept: "application/vnd.github+json",
    "User-Agent": "sve-kit-card-report",
    "X-GitHub-Api-Version": "2022-11-28",
  }
  const installation = await post(
    `${GITHUB}/app/installations/${installationId}/access_tokens`,
    { repositories: [CARD_REPORT_REPOSITORY.split("/")[1]], permissions: { issues: "write" } },
    { ...githubHeaders, Authorization: `Bearer ${jwt}` },
  )
  const accessToken = field(installation, "token")
  if (typeof accessToken !== "string" || !accessToken) throw new Error("Missing installation token")
  const card = report.cardNo ?? `Printing ${String(report.printingIntId)}`
  const body = [
    `- Card number: ${report.cardNo ?? "—"}`,
    ...(report.printingIntId === null ? [] : [`- Printing ID: ${String(report.printingIntId)}`]),
    `- Edition: ${report.edition.toUpperCase()}`,
    `- Interface language: ${report.uiLanguage}`,
    `- Data version: ${report.dataVersion}`,
    `- Issue type: ${report.type}`,
    ...(report.cardNo === null
      ? []
      : [`- Official card page: ${officialCardUrl(report.edition, report.cardNo)}`]),
    "",
    "### Description",
    "",
    plainTextMarkdown(report.description),
    ...(report.contact
      ? ["", "### Contact or nickname", "", plainTextMarkdown(report.contact)]
      : []),
  ].join("\n")
  const result = await post(
    `${GITHUB}/repos/${CARD_REPORT_REPOSITORY}/issues`,
    {
      title: `[${report.edition.toUpperCase()}] ${card} · ${report.type}`,
      body,
      labels: ["card-report"],
    },
    { ...githubHeaders, Authorization: `Bearer ${accessToken}` },
  )
  const number = field(result, "number")
  if (typeof number !== "number" || !Number.isSafeInteger(number) || number <= 0)
    throw new Error("Missing issue number")
  return number
}

export async function submitCardReport(request: Request, env: CardReportEnv): Promise<Response> {
  if (request.method !== "POST") return json({ error: "method-not-allowed" }, 405)
  const url = new URL(request.url)
  const origin = request.headers.get("Origin")
  if (origin !== null && origin !== url.origin) return json({ error: "invalid-report" }, 403)
  if (
    request.headers.get("Content-Type")?.split(";")[0]?.trim().toLowerCase() !== "application/json"
  )
    return json({ error: "invalid-report" }, 415)
  const report = await readReport(request)
  if (!report) return json({ error: "invalid-report" }, 400)

  const secret = env.TURNSTILE_SECRET
  const appId = env.CARD_REPORT_GITHUB_APP_ID
  const installationId = env.CARD_REPORT_GITHUB_INSTALLATION_ID
  const privateKey = env.CARD_REPORT_GITHUB_PRIVATE_KEY
  const limiter = env.CARD_REPORT_LIMITER
  const testSecret = secret === TEST_TURNSTILE_SECRET
  if (
    !secret?.trim() ||
    !appId ||
    !/^[0-9]+$/u.test(appId) ||
    !installationId ||
    !/^[0-9]+$/u.test(installationId) ||
    !privateKey?.trim() ||
    !limiter ||
    (testSecret && url.hostname !== "dev.svekit.app")
  )
    return json({ error: "not-configured" }, 503)

  try {
    const verification = await post(SITEVERIFY, { secret, response: report.token })
    const errors = field(verification, "error-codes")
    if (Array.isArray(errors)) {
      if (errors.includes("missing-input-secret") || errors.includes("invalid-input-secret"))
        return json({ error: "not-configured" }, 503)
      if (errors.includes("internal-error")) return json({ error: "server-error" }, 502)
    }
    if (
      field(verification, "success") !== true ||
      (!testSecret &&
        (field(verification, "hostname") !== url.hostname ||
          field(verification, "action") !== REPORT_TURNSTILE_ACTION))
    )
      return json({ error: "verification-failed" }, 403)
    const limit = await limiter.limit({ key: await sourceKey(request, secret) })
    if (!limit.success) return json({ error: "rate-limited" }, 429)
    const jwt = await signAppJwt(appId, privateKey)
    return json({ issueNumber: await createIssue(report, jwt, installationId) }, 201)
  } catch {
    return json({ error: "server-error" }, 502)
  }
}
