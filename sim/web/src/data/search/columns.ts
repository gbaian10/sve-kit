import type { JsonObject, JsonValue } from "../format-v3/json"

/** Nested cells use numeric references so the same text has only one retained string. */
export class StringPool {
  private readonly values: string[] = []
  private readonly ordinals = new Map<string, number>()
  private nodes = new Float64Array(1024)
  private used = 5
  private readonly stringNodes = new Map<number, number>()

  constructor() {
    this.nodes.set([0, 2, 0, 2, 1])
  }

  intern(value: string): number {
    const found = this.ordinals.get(value)
    if (found !== undefined) return found
    const ordinal = this.values.length
    this.values.push(value)
    this.ordinals.set(value, ordinal)
    return ordinal
  }

  get(ordinal: number): string {
    return this.values[ordinal] ?? ""
  }

  encode(value: JsonValue): number {
    let cells: number[]
    if (value === null) return 0
    if (typeof value === "boolean") return value ? 3 : 1
    if (typeof value === "number") cells = [1, value]
    else if (typeof value === "string") {
      const ordinal = this.intern(value)
      const existing = this.stringNodes.get(ordinal)
      if (existing !== undefined) return existing
      cells = [3, ordinal]
      this.stringNodes.set(ordinal, this.used)
    } else if (Array.isArray(value))
      cells = [4, value.length, ...value.map((item) => this.encode(item))]
    else
      cells = [
        5,
        Object.keys(value).length,
        ...Object.entries(value).flatMap(([key, item]) => [this.intern(key), this.encode(item)]),
      ]
    const offset = this.used
    if (offset + cells.length > this.nodes.length) {
      const grown = new Float64Array(Math.max(this.nodes.length * 2, offset + cells.length))
      grown.set(this.nodes)
      this.nodes = grown
    }
    this.nodes.set(cells, offset)
    this.used += cells.length
    return offset
  }

  decode(offset: number): JsonValue {
    const value = this.nodes[offset + 1] ?? 0
    switch (this.nodes[offset]) {
      case 1:
        return value
      case 2:
        return value === 1
      case 3:
        return this.get(value)
      case 4:
        return Array.from({ length: value }, (_, ordinal) =>
          this.decode(this.nodes[offset + 2 + ordinal] ?? 0),
        )
      case 5:
        return Object.fromEntries(
          Array.from({ length: value }, (_, ordinal) => [
            this.get(this.nodes[offset + 2 + ordinal * 2] ?? 0),
            this.decode(this.nodes[offset + 3 + ordinal * 2] ?? 0),
          ]),
        )
      default:
        return null
    }
  }

  seal(): void {
    this.nodes = this.nodes.slice(0, this.used)
    this.ordinals.clear()
    this.stringNodes.clear()
  }

  bufferBytes(): number {
    return this.nodes.byteLength
  }

  bytes(): number {
    return this.values.reduce((total, value) => total + value.length * 2, 0)
  }
}

interface Column {
  readonly kinds: Uint8Array
  readonly values: Float64Array
}
interface Block {
  readonly start: number
  readonly length: number
  readonly columns: ReadonlyMap<string, Column>
}

/** Fixed-size blocks avoid reallocating every previously parsed file during ingestion. */
export class Columns {
  private readonly pool: StringPool
  private readonly blocks: Block[] = []
  length = 0

  constructor(pool: StringPool) {
    this.pool = pool
  }

  append(rows: readonly JsonObject[]): void {
    if (rows.length === 0) return
    const names = new Set(rows.flatMap((row) => Object.keys(row)))
    const columns = new Map<string, Column>()
    for (const name of names) {
      const kinds = new Uint8Array(rows.length)
      const values = new Float64Array(rows.length)
      rows.forEach((row, ordinal) => {
        const value = row[name]
        if (value === null || value === undefined) return
        if (typeof value === "number") {
          kinds[ordinal] = 1
          values[ordinal] = value
        } else if (typeof value === "boolean") {
          kinds[ordinal] = 2
          values[ordinal] = Number(value)
        } else {
          kinds[ordinal] = typeof value === "string" ? 3 : 4
          values[ordinal] =
            typeof value === "string" ? this.pool.intern(value) : this.pool.encode(value)
        }
      })
      columns.set(name, { kinds, values })
    }
    this.blocks.push({ start: this.length, length: rows.length, columns })
    this.length += rows.length
  }

  value(ordinal: number, name: string): JsonValue {
    let low = 0
    let high = this.blocks.length - 1
    while (low <= high) {
      const middle = (low + high) >>> 1
      const block = this.blocks[middle]
      if (!block) break
      if (ordinal < block.start) high = middle - 1
      else if (ordinal >= block.start + block.length) low = middle + 1
      else {
        const column = block.columns.get(name)
        const offset = ordinal - block.start
        const value = column?.values[offset] ?? 0
        switch (column?.kinds[offset]) {
          case 1:
            return value
          case 2:
            return value === 1
          case 3:
            return this.pool.get(value)
          case 4:
            return this.pool.decode(value)
          default:
            return null
        }
      }
    }
    return null
  }

  row(ordinal: number): JsonObject {
    const block = this.blocks.find(
      (candidate) => ordinal >= candidate.start && ordinal < candidate.start + candidate.length,
    )
    return Object.fromEntries(
      [...(block?.columns.keys() ?? [])].map((name) => [name, this.value(ordinal, name)]),
    )
  }

  rows(): JsonObject[] {
    return Array.from({ length: this.length }, (_, ordinal) => this.row(ordinal))
  }

  bytes(): number {
    return this.blocks.reduce(
      (total, block) =>
        total +
        [...block.columns.values()].reduce(
          (size, column) => size + column.kinds.byteLength + column.values.byteLength,
          0,
        ),
      0,
    )
  }
}
