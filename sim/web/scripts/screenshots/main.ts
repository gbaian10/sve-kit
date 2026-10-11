// Manual dev screenshots use real data; see README for invocation and acceptance limits.
// Reports and stdout omit card wording; screenshot PNGs retain displayed official content.
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs"
import path from "node:path"
import { parseArgs } from "node:util"

import { type Browser, chromium, type Page } from "playwright"

import { VIEW_MODES } from "../../src/domain/query/model"
import { UI_LANGUAGES, type UiLanguage } from "../../src/i18n/languages"

const THEMES = ["dark", "light"] as const
const WIDTHS = [390, 820, 1180, 1440] as const
type Theme = (typeof THEMES)[number]
type Width = (typeof WIDTHS)[number]

interface Shot {
  readonly file: string
  readonly page: "cards" | "card"
  readonly subject: string
  readonly theme: Theme
  readonly language: UiLanguage
  readonly width: Width
  readonly height: number
  /** Viewport y of the first effect-text line on a card page (design 06: ≈300 at 390). */
  readonly effectTop: number | null
  readonly horizontalScroll: number
  readonly overflow: readonly string[]
  readonly consoleErrors: number
  /** List pages: visible slots still loading 1 s after a fast two-screen scroll (prefetch check). */
  readonly scrollLoadingSlots: number | null
  /** Wall-clock ms of this context's full load until the data was ready; a dev-server number, not a budget. */
  readonly readyMs: number
}

const log = (line: string) => process.stdout.write(`${line}\n`)

const { values } = parseArgs({
  options: {
    base: { type: "string", default: "http://localhost:5173" },
    out: { type: "string" },
    card: { type: "string", multiple: true, default: [] },
    query: { type: "string", default: "bp01" },
    views: { type: "string", default: VIEW_MODES.join(",") },
    themes: { type: "string", default: THEMES.join(",") },
    langs: { type: "string", default: UI_LANGUAGES.join(",") },
    widths: { type: "string", default: WIDTHS.join(",") },
  },
})
if (values.out === undefined) throw new Error("--out <dir> is required")
const out = path.resolve(values.out)
mkdirSync(out, { recursive: true })
const pick = <T extends string | number>(all: readonly T[], text: string): T[] =>
  all.filter((item) => text.split(",").includes(String(item)))
const themes = pick(THEMES, values.themes)
const languages = pick(UI_LANGUAGES, values.langs)
const widths = pick(WIDTHS, values.widths)
const views = pick(VIEW_MODES, values.views)

// Phone and wide phone keep a phone height; tablet landscape and desktop get a desktop height.
const heightOf = (width: Width): number => (width < 1000 ? 844 : 900)

// This file compiles without the DOM lib (tsconfig.node), so browser-side code is passed as
// source text and the returned shape is declared here.
type Geometry = Pick<Shot, "effectTop" | "horizontalScroll" | "overflow">

async function waitReady(page: Page): Promise<void> {
  // Ready = the dev badge carries the data version (title); loading and errors leave it empty.
  await page.waitForFunction(
    `Boolean(document.querySelector("[data-manifest-hash]")?.getAttribute("title"))`,
    undefined,
  )
}

// Below-fold slots are intentionally unloaded, so only visible image slots count here.
const VISIBLE_LOADING = `[...document.querySelectorAll("[data-image-state]")]
  .filter((slot) => { const r = slot.getBoundingClientRect(); return r.bottom > 0 && r.top < innerHeight && slot.dataset.imageState === "loading" })
  .length`

const PENDING_COUNT = `[...document.querySelectorAll("[data-image-state]")]
  .filter((slot) => slot.getBoundingClientRect().top < innerHeight && slot.dataset.imageState === "loading")
  .length + document.querySelectorAll("[data-result-key] [role=status]").length`

// A stalled count must not block screenshots indefinitely when metadata work delays a slot.
async function settleImages(page: Page): Promise<void> {
  await page.waitForTimeout(500)
  let previous = -1
  let stableSince = Date.now()
  const started = Date.now()
  for (;;) {
    const pending = await page.evaluate<number>(PENDING_COUNT)
    if (pending === 0) break
    if (pending !== previous) {
      previous = pending
      stableSince = Date.now()
    } else if (Date.now() - stableSince > 10_000) break
    if (Date.now() - started > 30_000) break
    await page.waitForTimeout(500)
  }
  await page.waitForTimeout(300)
}

const MEASURE = `(() => {
  const first = document.querySelector('[data-field="effect"] p')
  const html = document.documentElement
  return {
    effectTop: first ? Math.round(first.getBoundingClientRect().top) : null,
    horizontalScroll: Math.max(0, html.scrollWidth - html.clientWidth),
    overflow: (window.__sveOverflowAudit?.() ?? []).map(
      (problem) => problem.kind + " <" + problem.tag + "> " + String(problem.overflow) + "px",
    ),
  }
})()`

async function measure(page: Page): Promise<Geometry> {
  return page.evaluate<Geometry>(MEASURE)
}

async function capture(
  browser: Browser,
  theme: Theme,
  language: UiLanguage,
  width: Width,
): Promise<{ shots: Shot[]; dataVersion: string | null; manifestHash: string | null }> {
  const height = heightOf(width)
  const context = await browser.newContext({ viewport: { width, height }, deviceScaleFactor: 1 })
  await context.addInitScript(
    (prefs) => {
      localStorage.setItem("sve-kit:prefs", JSON.stringify(prefs))
    },
    { v: 1, theme, uiLanguage: language },
  )
  const page = await context.newPage()
  let consoleErrors = 0
  page.on("console", (message) => {
    if (message.type() === "error") consoleErrors += 1
  })
  page.on("pageerror", () => {
    consoleErrors += 1
  })
  const shots: Shot[] = []
  const tag = `${theme}-${language}-${String(width)}`
  const shoot = async (file: string, pageKind: Shot["page"], subject: string) => {
    await settleImages(page)
    await page.screenshot({ path: path.join(out, file) })
    const geometry = await measure(page)
    let scrollLoadingSlots: number | null = null
    if (pageKind === "cards") {
      await page.evaluate(`scrollTo(0, innerHeight * 2)`)
      await page.waitForTimeout(1000)
      scrollLoadingSlots = await page.evaluate<number>(VISIBLE_LOADING)
      await page.evaluate(`scrollTo(0, 0)`)
      await page.waitForTimeout(300)
    }
    shots.push({
      file,
      page: pageKind,
      subject,
      theme,
      language,
      width,
      height,
      ...geometry,
      consoleErrors,
      readyMs,
      scrollLoadingSlots,
    })
    log(`  ${file}: ready ${String(readyMs)}ms`)
    consoleErrors = 0
  }
  // The first page is a full load (data download included); later pages navigate inside the
  // app through the router's popstate listener, so the loaded snapshot is reused like a tap would.
  let readyMs = 0
  let loaded = false
  const visit = async (pathname: string) => {
    if (!loaded) {
      const started = Date.now()
      await page.goto(`${values.base}${pathname}`)
      // A busy dev server can miss the first window; one reload before giving the config up.
      await waitReady(page).catch(async () => {
        await page.reload()
        await waitReady(page)
      })
      const badge = page.locator("[data-manifest-hash]")
      const dataVersion = await badge.getAttribute("title")
      const manifestHash = await badge.getAttribute("data-manifest-hash")
      const prior = report.dataVersion === null ? previous : report
      if (
        prior &&
        prior.shots.length > 0 &&
        (prior.base !== values.base ||
          prior.browser !== `chromium ${browser.version()}` ||
          prior.dataVersion !== dataVersion ||
          prior.manifestHash !== manifestHash)
      ) {
        await context.close()
        throw new Error(
          "Screenshot sources differ from the existing report; use an empty --out directory",
        )
      }
      readyMs = Date.now() - started
      loaded = true
      return
    }
    await page.evaluate(
      `history.pushState({}, "", ${JSON.stringify(pathname)}); dispatchEvent(new PopStateEvent("popstate"))`,
    )
    await waitReady(page)
  }
  for (const view of views) {
    await visit(`/cards?q=${encodeURIComponent(values.query)}&view=${view}`)
    // An in-app view switch renders after the busy main thread frees up; wait for that view's list.
    await page.waitForSelector(`[data-view="${view}"] [data-result-key], [role=alert]`)
    await shoot(`cards-${view}-${values.query}-${tag}.png`, "cards", `${view} ${values.query}`)
  }
  for (const cardNo of values.card) {
    await visit(`/cards/${encodeURIComponent(cardNo)}`)
    // A card with pending wording has no effect section; the header (name) is the ready signal.
    await page.waitForSelector("article h2, [role=alert]")
    await page
      .waitForSelector('[data-field="effect"] p:not([role])', { timeout: 20_000 })
      .catch(() => undefined)
    await shoot(`card-${cardNo}-${tag}.png`, "card", cardNo)
  }
  const badge = page.locator("[data-manifest-hash]")
  const dataVersion = await badge.getAttribute("title")
  const manifestHash = await badge.getAttribute("data-manifest-hash")
  await context.close()
  return { shots, dataVersion, manifestHash }
}

const browser = await chromium.launch()
interface Report {
  base: string
  browser: string
  dataVersion: string | null
  manifestHash: string | null
  shots: Shot[]
}
// A partial re-run (one page kind, a few configurations) replaces only the shots it retakes, so
// the report keeps describing every file in the directory.
const previous: Report | null = existsSync(path.join(out, "report.json"))
  ? (JSON.parse(readFileSync(path.join(out, "report.json"), "utf8")) as Report)
  : null
const report: Report = {
  base: values.base,
  browser: `chromium ${browser.version()}`,
  dataVersion: null,
  manifestHash: null,
  shots: previous?.shots ?? [],
}
const record = (shot: Shot) => {
  report.shots = [...report.shots.filter((old) => old.file !== shot.file), shot]
}
try {
  for (const theme of themes)
    for (const language of languages)
      for (const width of widths) {
        const result = await capture(browser, theme, language, width)
        report.dataVersion ??= result.dataVersion
        report.manifestHash ??= result.manifestHash
        for (const shot of result.shots) record(shot)
        // Written after every configuration, so an interrupted run still leaves its report.
        writeFileSync(path.join(out, "report.json"), JSON.stringify(report, null, 2))
        process.stdout.write(
          `${theme} ${language} ${String(width)}: ${String(result.shots.length)} shots\n`,
        )
      }
} finally {
  await browser.close()
}

const problems = report.shots.filter(
  (shot) => shot.horizontalScroll > 0 || shot.overflow.length > 0 || shot.consoleErrors > 0,
)
log(
  `${String(report.shots.length)} screenshots in ${out}; ${String(problems.length)} with findings`,
)
for (const shot of problems)
  log(
    `  ${shot.file}: hscroll=${String(shot.horizontalScroll)} overflow=[${shot.overflow.join("; ")}] consoleErrors=${String(shot.consoleErrors)}`,
  )
