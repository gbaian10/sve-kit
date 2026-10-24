import { screen, within } from "@testing-library/react"
import { afterEach, beforeEach, expect, it, vi } from "vitest"

import { buildSnapshot } from "../../../scripts/fixture/build"
import { routes } from "../../app/routes"
import { arrayValue, type JsonObject, type JsonValue, objectValue, stringValue } from "../../data"
import { prefsStore } from "../../settings"
import { renderRoutes } from "../../test-utils"
import { TestSearchWorker } from "../../test-utils/search-worker"
beforeEach(() => vi.stubGlobal("Worker", TestSearchWorker))

const built = await buildSnapshot({ encodeImage: () => Promise.resolve(new Uint8Array([1])) })
const hash = async (bytes: Uint8Array) =>
  `sha256:${Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256", bytes.slice().buffer)), (value) => value.toString(16).padStart(2, "0")).join("")}`

afterEach(() => vi.unstubAllGlobals())
it.each(["hash mismatch", "invalid JSON", "missing card ID"])(
  "shows a load failure for %s",
  async (failure) => {
    const files = new Map(built.files)
    const manifest = structuredClone(built.manifest)
    if (failure === "missing card ID") {
      const file = (manifest["files"] as JsonObject[]).find(
        (file) =>
          file["role"] === "bootstrap" &&
          file["region"] === "jp" &&
          (file["row_counts"] as JsonValue[][]).some((count) => count[0] === "card"),
      )
      if (!file) throw new Error("missing synthetic card file")
      const body = objectValue(
        JSON.parse(new TextDecoder().decode(files.get(stringValue(file["path"])))) as JsonObject,
      )
      const fragment = arrayValue(arrayValue(objectValue(body["tables"])["card"])[0])
      arrayValue(arrayValue(fragment[4])[0])[0] = null
      const bytes = new TextEncoder().encode(JSON.stringify(body))
      file["sha256"] = await hash(bytes)
      file["bytes"] = bytes.length
      files.set(stringValue(file["path"]), bytes)
    }
    const bytes = new TextEncoder().encode(
      failure === "invalid JSON" ? "{" : JSON.stringify(manifest),
    )
    const digest = await hash(bytes)
    const path = `snapshots/manifests/${digest.slice(7)}.json`
    files.set(path, failure === "hash mismatch" ? new Uint8Array([1]) : bytes)
    const index = objectValue(
      JSON.parse(
        new TextDecoder().decode(files.get("snapshots/versions/index.json")),
      ) as JsonObject,
    )
    objectValue(index["current"])["manifest_path"] = path
    objectValue(index["current"])["manifest_sha256"] = digest
    files.set("snapshots/versions/index.json", new TextEncoder().encode(JSON.stringify(index)))
    vi.stubGlobal("fetch", (input: RequestInfo | URL) => {
      const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url
      const body = files.get(url.replace(/^.*\/cdn\//u, ""))
      return Promise.resolve(
        body ? new Response(body.slice().buffer) : new Response(null, { status: 404 }),
      )
    })
    prefsStore.set({ uiLanguage: "zh-TW" })
    await renderRoutes(routes, { initialEntries: ["/cards"] })
    const alert = await screen.findByRole("alert")
    expect(alert).toHaveTextContent("載入失敗")
    expect(within(alert).getByRole("button", { name: "重試" })).toBeVisible()
    expect(document.querySelector("a[data-result-key]")).toBeNull()
  },
)

it("shows how many startup files have arrived during a first download", async () => {
  const rulesTables = ["rules_profile", "restriction", "rules_name", "face_rules_name"]
  // Restriction rules ride the startup download so the filter never answers from partial data.
  const startup = (built.manifest["files"] as JsonObject[]).filter(
    (file) =>
      (file["region"] === "jp" || file["region"] === null) &&
      (file["role"] === "config" ||
        file["purpose"] === "catalog" ||
        (file["purpose"] === "engine" &&
          (file["row_counts"] as JsonValue[][]).some((count) =>
            rulesTables.includes(stringValue(count[0])),
          ))),
  )
  const held = stringValue(startup[0]?.["path"])
  // The page shares one client with the cases above, which published other indexes at the same
  // revision; a newer revision is a legitimate new version rather than a changed old one.
  const index = objectValue(
    JSON.parse(
      new TextDecoder().decode(built.files.get("snapshots/versions/index.json")),
    ) as JsonObject,
  )
  index["revision"] = Number(index["revision"]) + 1
  const files = new Map(built.files)
  files.set("snapshots/versions/index.json", new TextEncoder().encode(JSON.stringify(index)))
  let release = () => {}
  const gate = new Promise<void>((resolve) => {
    release = resolve
  })
  vi.stubGlobal("fetch", async (input: RequestInfo | URL) => {
    const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url
    const key = url.replace(/^.*\/cdn\//u, "")
    if (key === held) await gate
    const body = files.get(key)
    return body ? new Response(body.slice().buffer) : new Response(null, { status: 404 })
  })
  prefsStore.set({ uiLanguage: "zh-TW" })
  await renderRoutes(routes, { initialEntries: ["/cards"] })
  const bar = await screen.findByRole("progressbar", { name: "卡片資料下載進度" })
  await vi.waitFor(() => {
    expect(bar.getAttribute("aria-valuenow")).toBe(String(startup.length - 2))
  })
  expect(bar.getAttribute("aria-valuemax")).toBe(String(startup.length - 1))
  expect(
    screen.getByText(`${String(startup.length - 2)}/${String(startup.length - 1)}`),
  ).toBeTruthy()
  release()
  await vi.waitFor(() => {
    expect(screen.queryByRole("progressbar")).toBeNull()
  })
})
