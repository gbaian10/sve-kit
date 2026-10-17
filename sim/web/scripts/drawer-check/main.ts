// Manual browser check of the card drawer against a running dev server with real data: the
// drawer pushes the frame instead of covering it, a saved width fits a narrower window, printing
// links keep the result cell, history keeps one card entry, and closing puts the opening cell back.
// Reports geometry and URLs only; no card wording.
import { mkdirSync, writeFileSync } from "node:fs"
import path from "node:path"
import { parseArgs } from "node:util"

import { type BrowserContext, chromium, type Page } from "playwright"

const { values } = parseArgs({
  options: {
    base: { type: "string", default: "https://127.0.0.1:5793" },
    out: { type: "string" },
  },
})
const out = values.out === undefined ? undefined : path.resolve(values.out)
if (out !== undefined) mkdirSync(out, { recursive: true })
const log = (line: string) => process.stdout.write(`${line}\n`)

interface Check {
  readonly name: string
  readonly ok: boolean
  readonly detail: string
}
const checks: Check[] = []
function expect(name: string, ok: boolean, detail: string) {
  checks.push({ name, ok, detail })
  log(`${ok ? "ok  " : "FAIL"} ${name}: ${detail}`)
}

interface Rect {
  x: number
  y: number
  width: number
  height: number
}
interface Geometry {
  viewport: { width: number; height: number }
  drawer: Rect | null
  main: Rect | null
  avatar: Rect | null
  scrollWidth: number
  clientWidth: number
  drawerWidthVar: string
  framePaddingRight: number
}
// This file compiles without the DOM lib (tsconfig.node), so browser-side code is source text.
const GEOMETRY = `(() => {
  const rect = (el) => { if (!el) return null; const r = el.getBoundingClientRect(); return { x: r.x, y: r.y, width: r.width, height: r.height } }
  const main = document.getElementById("main")
  const frame = main ? main.parentElement : null
  return {
    viewport: { width: innerWidth, height: innerHeight },
    drawer: rect(document.querySelector('aside[data-card-overlay][data-state="open"]')),
    main: rect(main),
    avatar: rect(document.querySelector('header button[aria-label]')),
    scrollWidth: document.documentElement.scrollWidth,
    clientWidth: document.documentElement.clientWidth,
    drawerWidthVar: document.documentElement.style.getPropertyValue("--drawer-width"),
    framePaddingRight: frame ? parseFloat(getComputedStyle(frame).paddingRight) : -1,
  }
})()`
const geometry = (page: Page) => page.evaluate<Geometry>(GEOMETRY)

interface Focus {
  tag: string
  key: string | null
  scrollY: number
  selected: string | null
  navigation: number
  card: string | null
}
const FOCUS = `(() => ({
  tag: document.activeElement ? document.activeElement.tagName : "",
  key: document.activeElement ? document.activeElement.getAttribute("data-result-key") : null,
  scrollY: scrollY,
  selected: (document.querySelector("a[data-result-key][aria-current]") || { getAttribute: () => null }).getAttribute("data-result-key"),
  navigation: document.querySelectorAll("[data-card-overlay] [data-card-navigation]").length,
  card: new URL(location.href).searchParams.get("card"),
}))()`
const focus = (page: Page) => page.evaluate<Focus>(FOCUS)

async function ready(page: Page) {
  await page.waitForFunction(
    `Boolean(document.querySelector("[data-manifest-hash]")?.getAttribute("title"))`,
    undefined,
    { timeout: 60_000 },
  )
}
async function cardLoaded(page: Page) {
  await page.waitForSelector('[data-card-overlay] article[aria-busy="false"]', { timeout: 60_000 })
}
async function settled(page: Page) {
  // The frame's padding and the drawer animate for 200ms.
  await page.waitForTimeout(400)
}
async function context(
  width: number,
  prefs: Record<string, unknown>,
): Promise<{ context: BrowserContext; page: Page }> {
  const created = await browser.newContext({
    viewport: { width, height: 900 },
    deviceScaleFactor: 1,
    ignoreHTTPSErrors: true,
  })
  await created.addInitScript(
    (stored) => {
      localStorage.setItem("sve-kit:prefs", JSON.stringify(stored))
    },
    {
      v: 1,
      theme: "dark",
      uiLanguage: "zh-TW",
      cardEdition: "jp",
      cardOpenMode: "panel",
      infiniteScroll: false,
      ...prefs,
    },
  )
  const page = await created.newPage()
  return { context: created, page }
}
async function openFirst(page: Page, pathname: string) {
  await page.goto(`${values.base}${pathname}`)
  await ready(page)
  const first = page.locator("a[data-result-key]").first()
  await first.waitFor({ timeout: 60_000 })
  await first.click()
  await cardLoaded(page)
  await settled(page)
}
const near = (a: number, b: number, tolerance = 2) => Math.abs(a - b) <= tolerance

const browser = await chromium.launch()
try {
  // The drawer pushes the frame: main and the avatar end where the drawer starts, nothing overflows.
  {
    const { context: ctx, page } = await context(1440, {})
    await openFirst(page, "/cards?view=grid")
    const g = await geometry(page)
    const drawer = g.drawer
    expect(
      "drawer spans the window height",
      drawer !== null && drawer.y === 0 && near(drawer.height, g.viewport.height),
      JSON.stringify(drawer),
    )
    expect(
      "frame padding equals the drawer width",
      drawer !== null &&
        near(g.framePaddingRight, drawer.width) &&
        g.drawerWidthVar === `${String(Math.round(drawer.width))}px`,
      `padding ${String(g.framePaddingRight)} drawer ${String(drawer?.width)} var ${g.drawerWidthVar}`,
    )
    expect(
      "main content ends at the drawer",
      drawer !== null && g.main !== null && g.main.x + g.main.width <= drawer.x + 1,
      `main right ${String(g.main ? g.main.x + g.main.width : null)} drawer x ${String(drawer?.x)}`,
    )
    expect(
      "avatar sits left of the drawer",
      drawer !== null && g.avatar !== null && g.avatar.x + g.avatar.width <= drawer.x + 1,
      JSON.stringify(g.avatar),
    )
    expect(
      "no horizontal overflow",
      g.scrollWidth <= g.clientWidth,
      `${String(g.scrollWidth)} / ${String(g.clientWidth)}`,
    )
    const second = page.locator("a[data-result-key]").nth(1)
    const secondKey = await second.getAttribute("data-result-key")
    await second.click()
    // The marker moves with the next render; the URL alone is ahead of it for a moment.
    await page
      .waitForFunction(
        `document.querySelector("a[data-result-key][aria-current]")?.getAttribute("data-result-key") === ${JSON.stringify(secondKey)}`,
        undefined,
        { timeout: 10_000 },
      )
      .catch(() => undefined)
    await cardLoaded(page)
    const after = await focus(page)
    expect(
      "a result beside the drawer opens in it",
      after.card !== null && after.selected === secondKey,
      JSON.stringify(after),
    )
    await ctx.close()
  }
  // F1: a saved width wider than a narrower window allows is cut to what the window leaves.
  {
    const { context: ctx, page } = await context(1800, { drawerWidth: 1200 })
    await openFirst(page, "/cards?view=grid")
    for (const width of [1180, 1000]) {
      await page.setViewportSize({ width, height: 900 })
      await settled(page)
      const g = await geometry(page)
      const drawer = g.drawer
      expect(
        `saved width fits a ${String(width)} window`,
        drawer !== null &&
          drawer.x >= 0 &&
          near(drawer.x + drawer.width, width) &&
          drawer.width <= width - 400 &&
          near(g.framePaddingRight, drawer.width) &&
          g.scrollWidth <= g.clientWidth,
        `drawer ${JSON.stringify(drawer)} padding ${String(g.framePaddingRight)} scrollWidth ${String(g.scrollWidth)}`,
      )
      expect(
        `results stay usable at ${String(width)}`,
        g.main !== null && g.main.width >= 300,
        `main width ${String(g.main?.width)}`,
      )
    }
    await page.setViewportSize({ width: 900, height: 900 })
    await settled(page)
    const modal = await page.evaluate<number>(`document.querySelectorAll("dialog[open]").length`)
    const g = await geometry(page)
    expect(
      "below the room for a drawer the card becomes a modal",
      modal === 1 && g.drawer === null && g.drawerWidthVar === "",
      `dialogs ${String(modal)} drawer ${JSON.stringify(g.drawer)}`,
    )
    await ctx.close()
  }
  // F2: another printing of the card keeps the result cell, previous/next and the return focus.
  {
    const { context: ctx, page } = await context(1440, {})
    await openFirst(page, "/cards?view=grid&unit=card")
    const before = await focus(page)
    const pill = page
      .locator('[data-card-overlay] ul[aria-label="JP"] a:not([aria-current])')
      .first()
    await pill.waitFor({ timeout: 30_000 })
    await pill.click()
    await cardLoaded(page)
    await page.waitForTimeout(500)
    const after = await focus(page)
    expect(
      "printing change keeps the result cell",
      after.selected === before.selected && after.card !== before.card,
      JSON.stringify({ before, after }),
    )
    expect(
      "printing change keeps previous/next",
      after.navigation === 2,
      `buttons ${String(after.navigation)}`,
    )
    await page.keyboard.press("Escape")
    await page.waitForFunction(`!new URL(location.href).searchParams.has("card")`)
    await settled(page)
    const closed = await focus(page)
    expect(
      "closing after a printing change focuses the opening cell",
      closed.key === before.selected,
      JSON.stringify(closed),
    )
    await ctx.close()
  }
  // F3: a sort change keeps the card, back closes it, back again restores the old conditions.
  {
    const { context: ctx, page } = await context(1440, {})
    await openFirst(page, "/cards?view=grid")
    await page.getByRole("combobox", { name: "排序" }).selectOption("cost")
    await page.waitForFunction(`new URL(location.href).searchParams.get("sort") === "cost"`)
    await cardLoaded(page)
    const sorted = await focus(page)
    expect("sort change keeps the card open", sorted.card !== null, JSON.stringify(sorted))
    await page.goBack()
    await page.waitForFunction(`!new URL(location.href).searchParams.has("card")`)
    await settled(page)
    const backOnce = await page.evaluate<string>(`location.search`)
    expect(
      "back closes the card and keeps the sort",
      backOnce === "?view=grid&sort=cost" || backOnce === "?sort=cost&view=grid",
      backOnce,
    )
    await page.goBack()
    await page.waitForTimeout(400)
    const backTwice = await page.evaluate<string>(`location.search`)
    expect(
      "back again is the old list without a card",
      !backTwice.includes("sort=") && !backTwice.includes("card="),
      backTwice,
    )
    await ctx.close()
  }
  // F4: a card opened from the second page comes back to the same place with focus on its cell.
  {
    const { context: ctx, page } = await context(1440, {})
    await page.goto(`${values.base}/cards?view=grid`)
    await ready(page)
    await page.locator("a[data-result-key]").first().waitFor({ timeout: 60_000 })
    await page.getByRole("button", { name: "載入更多", exact: true }).click()
    await page.waitForFunction(`document.querySelectorAll("a[data-result-key]").length > 60`)
    const target = page.locator("a[data-result-key]").nth(65)
    await target.scrollIntoViewIfNeeded()
    await page.waitForTimeout(800)
    const key = await target.getAttribute("data-result-key")
    const before = await target.boundingBox()
    const scrollBefore = await page.evaluate<number>(`scrollY`)
    await target.click()
    await cardLoaded(page)
    await settled(page)
    await page.goBack()
    await page.waitForFunction(`!new URL(location.href).searchParams.has("card")`)
    await page.waitForTimeout(700)
    const after = await target.boundingBox()
    const closed = await focus(page)
    expect(
      "second-page card returns to the same viewport position",
      before !== null && after !== null && near(after.y, before.y, 4),
      `before y ${String(before?.y)} after y ${String(after?.y)} scroll ${String(scrollBefore)} → ${String(closed.scrollY)}`,
    )
    expect("focus is on the opening cell", closed.key === key, JSON.stringify(closed))
    await ctx.close()
  }
} finally {
  await browser.close()
}
const failed = checks.filter((check) => !check.ok)
if (out !== undefined)
  writeFileSync(
    path.join(out, "drawer-check.json"),
    JSON.stringify({ base: values.base, checks }, null, 2),
  )
log(`${String(checks.length)} checks, ${String(failed.length)} failed`)
process.exitCode = failed.length === 0 ? 0 : 1
