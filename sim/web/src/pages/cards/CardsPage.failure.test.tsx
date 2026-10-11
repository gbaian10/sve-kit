import { screen, within } from "@testing-library/react"
import { afterEach, expect, it, vi } from "vitest"

import { buildSnapshot } from "../../../scripts/fixture/build"
import { routes } from "../../app/routes"
import { arrayValue, type JsonObject, objectValue, stringValue } from "../../data"
import { prefsStore } from "../../settings"
import { renderRoutes } from "../../test-utils"

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
          (file["row_counts"] as JsonObject[]).some((count) => count["table"] === "card"),
      )
      if (!file) throw new Error("missing synthetic card file")
      const body = objectValue(
        JSON.parse(new TextDecoder().decode(files.get(stringValue(file["path"])))) as JsonObject,
      )
      const fragment = objectValue(arrayValue(objectValue(body["tables"])["card"])[0])
      arrayValue(arrayValue(fragment["rows"])[0])[0] = null
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
