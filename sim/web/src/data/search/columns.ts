import type { JsonObject, JsonValue } from "../format-v3/json"

/** Nested cells use numeric references so the same text has only one retained string. */
export class StringPool {
  private readonly values: string[] = []
  private readonly ordinals = new Map<string, number>()
  private nodes = new Uint32Array(1024)
  private used = 5
  private numbers = new Float64Array(32)
  private numberCount = 0
  private readonly stringNodes = new Map<number, number>()

  private readonly internIds: boolean

  constructor(internIds = true) {
    this.internIds = internIds
    this.nodes.set([0, 2, 0, 2, 1])
  }

  retain(value: string): number {
    const ordinal = this.values.length
    this.values.push(value)
    return ordinal
  }

  intern(value: string, identity = false): number {
    // Staging IDs are mostly unique; retaining their strings avoids a second large identity map.
    if (!this.internIds && identity) return this.retain(value)
    const found = this.ordinals.get(value)
    if (found !== undefined) return found
    const ordinal = this.retain(value)
    this.ordinals.set(value, ordinal)
    return ordinal
  }

  get(ordinal: number): string {
    return this.values[ordinal] ?? ""
  }

  encode(value: JsonValue, identity = false): number {
    let cells: number[]
    if (value === null) return 0
    if (typeof value === "boolean") return value ? 3 : 1
    if (typeof value === "number") {
      if (this.numberCount === this.numbers.length) {
        const grown = new Float64Array(this.numbers.length * 2)
        grown.set(this.numbers)
        this.numbers = grown
      }
      cells = [1, this.numberCount]
      this.numbers[this.numberCount++] = value
    } else if (typeof value === "string") {
      const ordinal = this.intern(value, identity)
      const existing = this.stringNodes.get(ordinal)
      if (existing !== undefined) return existing
      cells = [3, ordinal]
      if (this.internIds || !identity) this.stringNodes.set(ordinal, this.used)
    } else if (Array.isArray(value))
      cells = [4, value.length, ...value.map((item) => this.encode(item, identity))]
    else
      cells = [
        5,
        Object.keys(value).length,
        ...Object.entries(value).flatMap(([key, item]) => [
          this.intern(key),
          this.encode(item, key === "id" || key.endsWith("_id")),
        ]),
      ]
    const offset = this.used
    if (offset + cells.length > this.nodes.length) {
      const grown = new Uint32Array(Math.max(this.nodes.length * 2, offset + cells.length))
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
        return this.numbers[value] ?? 0
      case 2:
        return value === 1
      case 3:
        return this.get(value)
      case 4: {
        const items: JsonValue[] = []
        for (let ordinal = 0; ordinal < value; ordinal += 1)
          items.push(this.decode(this.nodes[offset + 2 + ordinal] ?? 0))
        return items
      }
      case 5: {
        // Plain assignment is safe: keys are schema columns, region/language codes and entity ids.
        const object: JsonObject = {}
        for (let ordinal = 0; ordinal < value; ordinal += 1)
          object[this.get(this.nodes[offset + 2 + ordinal * 2] ?? 0)] = this.decode(
            this.nodes[offset + 3 + ordinal * 2] ?? 0,
          )
        return object
      }
      default:
        return null
    }
  }

  seal(): void {
    this.nodes = this.nodes.slice(0, this.used)
    this.numbers = this.numbers.slice(0, this.numberCount)
    this.ordinals.clear()
    this.stringNodes.clear()
  }

  bufferBytes(): number {
    return this.nodes.byteLength + this.numbers.byteLength
  }

  bytes(): number {
    return this.values.reduce((total, value) => total + value.length * 2, 0)
  }
}

interface Column {
  readonly kinds: Uint8Array
  readonly values: Uint32Array
}
interface Block {
  readonly start: number
  readonly length: number
  readonly columns: ReadonlyMap<string, Column>
}

/** Each append keeps its own block, so ingesting a file never reallocates earlier files. */
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
      const values = new Uint32Array(rows.length)
      rows.forEach((row, ordinal) => {
        const value = row[name]
        if (value === null || value === undefined) return
        if (typeof value === "number") {
          if (Number.isInteger(value) && value >= 0 && value <= 0xffff_ffff) {
            kinds[ordinal] = 1
            values[ordinal] = value
          } else {
            kinds[ordinal] = 4
            values[ordinal] = this.pool.encode(value)
          }
        } else if (typeof value === "boolean") {
          kinds[ordinal] = 2
          values[ordinal] = Number(value)
        } else {
          kinds[ordinal] = typeof value === "string" ? 3 : 4
          values[ordinal] =
            typeof value === "string"
              ? this.pool.intern(value, name === "id" || name.endsWith("_id"))
              : this.pool.encode(value)
        }
      })
      columns.set(name, { kinds, values })
    }
    this.blocks.push({ start: this.length, length: rows.length, columns })
    this.length += rows.length
  }

  private block(ordinal: number): Block | undefined {
    let low = 0
    let high = this.blocks.length - 1
    while (low <= high) {
      const middle = (low + high) >>> 1
      const block = this.blocks[middle]
      if (!block) break
      if (ordinal < block.start) high = middle - 1
      else if (ordinal >= block.start + block.length) low = middle + 1
      else return block
    }
    return undefined
  }

  private cell(column: Column | undefined, offset: number): JsonValue {
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

  value(ordinal: number, name: string): JsonValue {
    const block = this.block(ordinal)
    return block ? this.cell(block.columns.get(name), ordinal - block.start) : null
  }

  // Rows are rebuilt on every lookup and query, so one block search serves all of a row's cells.
  private rowAt(block: Block, offset: number): JsonObject {
    const row: JsonObject = {}
    for (const [name, column] of block.columns) row[name] = this.cell(column, offset)
    return row
  }

  row(ordinal: number): JsonObject {
    const block = this.block(ordinal)
    return block ? this.rowAt(block, ordinal - block.start) : {}
  }

  *iterate(): Generator<JsonObject> {
    for (const block of this.blocks)
      for (let offset = 0; offset < block.length; offset += 1) yield this.rowAt(block, offset)
  }

  rows(): JsonObject[] {
    return [...this.iterate()]
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
