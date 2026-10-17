// @vitest-environment node
import { describe, expect, it } from "vitest"

import { Columns, StringPool } from "./columns"

describe("packed column completion", () => {
  it("reuses equal nested values with different object key order without growing the pool", () => {
    const pool = new StringPool()
    const source = new Columns(pool)
    source.append([{ nested: { labels: ["shared", null], amount: 1.5 } }])
    pool.seal()
    const bytes = pool.bufferBytes()
    pool.reopen()
    const completed = source.remap([
      [{ source: 0, changes: { nested: { amount: 1.5, labels: ["shared", null] } } }],
    ])
    pool.seal()
    expect(completed.rows()).toEqual(source.rows())
    expect(pool.bufferBytes()).toBe(bytes)
  })

  it("keeps absent nested keys distinct from explicit null and different key names", () => {
    const pool = new StringPool()
    const source = new Columns(pool)
    source.append([{ nested: {} }, { nested: { optional: null } }])
    const completed = source.remap([
      [
        { source: 0, changes: { nested: { optional: null } } },
        { source: 1, changes: { nested: { other: null } } },
      ],
    ])
    expect(completed.rows()).toEqual([{ nested: { optional: null } }, { nested: { other: null } }])
  })

  it("reorders batches, patches optional fields and preserves the source after restoration", () => {
    const pool = new StringPool()
    const source = new Columns(pool)
    source.append([
      { id: "first", nested: { name: "shared", items: [true, null] }, optional: "old" },
    ])
    source.append([{ id: "second", nested: { name: "shared", items: [false] } }])
    pool.seal()
    const before = source.rows()
    pool.reopen()
    const completed = source.remap([
      [{ source: 1, changes: { count: { value: 1.5 }, optional: null } }],
      [
        {
          source: 0,
          changes: {
            optional: undefined,
            id: "updated",
            nested: { items: [true, null], name: "shared" },
          },
        },
      ],
    ])
    pool.seal()
    const expected = [
      {
        id: "second",
        nested: { name: "shared", items: [false] },
        count: { value: 1.5 },
        optional: null,
      },
      { id: "updated", nested: { name: "shared", items: [true, null] } },
    ]
    expect(completed.rows()).toEqual(expected)
    expect(source.rows()).toEqual(before)
    const parts = pool.parts()
    const restored = StringPool.restore(parts.values, parts.nodes, parts.numbers)
    expect(Columns.restore(restored, completed.parts()).rows()).toEqual(expected)
  })

  it("reopens nested strings and can append the first number after sealing", () => {
    const pool = new StringPool()
    const before = pool.encode({ labels: ["same", "other"], flag: true })
    pool.seal()
    pool.reopen()
    const after = pool.encode({ labels: ["same", "new"], amount: 2.5 })
    pool.seal()
    expect(pool.decode(before)).toEqual({ labels: ["same", "other"], flag: true })
    expect(pool.decode(after)).toEqual({ labels: ["same", "new"], amount: 2.5 })
  })

  it("rejects an unknown source ordinal without changing existing rows", () => {
    const pool = new StringPool()
    const source = new Columns(pool)
    source.append([{ id: "first" }])
    expect(() => source.remap([[{ source: 1, changes: {} }]])).toThrow("out of range")
    expect(source.rows()).toEqual([{ id: "first" }])
  })
})
