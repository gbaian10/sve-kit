import { renderHook, waitFor } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import fixture from "../../../../tests/fixtures/snapshot-contract/v3/whole-name-native.json"
import { SnapshotError } from "../data"
import { annotatedOrigin } from "../test-utils/annotated-snapshot"
import { useAnnotatedText } from "./snapshot"

const owner = { kind: "face_revision", id: "revision" }

describe("useAnnotatedText", () => {
  it("keeps every purpose pending until the exact owner resolves, then reports each from the value", async () => {
    const { client } = await annotatedOrigin(fixture)
    await client.load()
    const { result } = renderHook(() => useAnnotatedText(client, owner, "name", "zh-Hant", "jp"))
    expect(result.current.readiness).toMatchObject({
      owner,
      edition: "jp",
      ordinal: null,
      field: "name",
      language: "zh-Hant",
      basic: "pending",
      source: "pending",
      annotations: "pending",
    })
    expect(result.current.value).toBeNull()
    await waitFor(() => {
      expect(result.current.readiness.basic).toBe("ready")
    })
    // A whole-name annotation on the translation: annotations ready; own_source has no JP source.
    expect(result.current.readiness).toMatchObject({ source: "none", annotations: "ready" })
    expect(result.current.readiness.dataVersion).toBeTypeOf("string")
    expect(result.current.readiness.manifestHash).toMatch(/^sha256:/)
    expect(result.current.readiness.generation).toBeTypeOf("number")
    expect(result.current.value?.translated?.annotation).not.toBeNull()
  })

  it("renders a text resolved earlier for the snapshot on the first pass", async () => {
    const { client } = await annotatedOrigin(fixture)
    await client.load()
    const first = renderHook(() => useAnnotatedText(client, owner, "name", "zh-Hant", "jp"))
    await waitFor(() => {
      expect(first.result.current.readiness.basic).toBe("ready")
    })
    first.unmount()
    // Switching back to a view mounts the hook again; it must not flash the pending state.
    const again = renderHook(() => useAnnotatedText(client, owner, "name", "zh-Hant", "jp"))
    expect(again.result.current.readiness.basic).toBe("ready")
    expect(again.result.current.value).toBe(first.result.current.value)
  })

  it("reports an unknown field as none for every purpose, not as an error", async () => {
    const { client } = await annotatedOrigin(fixture)
    await client.load()
    const resolve = vi.fn().mockResolvedValue(null)
    const { result } = renderHook(() =>
      useAnnotatedText(client, owner, "flavor", "zh-Hant", "jp", {
        resolve,
        peek: () => undefined,
      }),
    )
    await waitFor(() => {
      expect(result.current.readiness.basic).toBe("none")
    })
    expect(result.current.readiness).toMatchObject({ source: "none", annotations: "none" })
    expect(result.current.readiness.error).toBeUndefined()
  })

  it.each([
    [
      new SnapshotError("public-annotation/reference", "bad"),
      "validation",
      "public-annotation/reference",
    ],
    [new Error("offline"), "load", undefined],
  ])(
    "distinguishes a rejected closure from a transport failure: %s",
    async (thrown, kind, code) => {
      const { client } = await annotatedOrigin(fixture)
      await client.load()
      const resolve = vi.fn().mockRejectedValue(thrown)
      const { result } = renderHook(() =>
        useAnnotatedText(client, owner, "name", "zh-Hant", "jp", {
          resolve,
          peek: () => undefined,
        }),
      )
      await waitFor(() => {
        expect(result.current.readiness.basic).toBe("error")
      })
      expect(result.current.readiness).toMatchObject({
        source: "error",
        annotations: "error",
        error: { kind, ...(code === undefined ? {} : { code }) },
      })
      expect(result.current.value).toBeNull()
    },
  )
})
