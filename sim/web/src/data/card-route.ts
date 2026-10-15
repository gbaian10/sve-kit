import {
  CardRouteError,
  resolveRoute,
  type RouteAlias,
  routeFold,
  type RoutePrinting,
} from "../domain/cardRoute"
import type { Region } from "../domain/search"
import { type Catalog, catalogOf } from "./catalog"
import type { LoadedSnapshot, SnapshotClient } from "./client"
import { arrayValue, type JsonObject, stringValue } from "./format-v3/json"
import { mergedRows } from "./format-v3/reader"
import { createLocator, homeSetOwner } from "./locator"

export interface CardRouteInput {
  readonly namespace: "official" | "provisional"
  readonly key: string
  readonly edition?: Region
  readonly hint?: Omit<RoutePrinting, "cardNo" | "cardNoState" | "intId">
}
export interface LocatedCard {
  readonly printing: RoutePrinting
  readonly catalog: Catalog
  readonly snapshot: LoadedSnapshot
}
const routingTables = new Set(["card_route_alias", "route_override"])
const includesTable = (file: JsonObject, tables: ReadonlySet<string>) =>
  arrayValue(file["row_counts"]).some((count) => tables.has(stringValue(arrayValue(count)[0])))

/** Uses the existing catalog locators; a non-default printing is loaded from its own owner. */
export async function locateCard(
  client: SnapshotClient,
  input: CardRouteInput,
): Promise<LocatedCard> {
  const edition = input.edition ?? input.hint?.region
  await (edition ? (client.selectRegion?.(edition) ?? client.load()) : client.load())
  const initial = client.snapshot()
  if (!initial) throw new CardRouteError("reference", "card snapshot is unavailable")
  let snapshot: LoadedSnapshot = initial
  const extra = input.hint
    ? []
    : (
        await Promise.all(
          [...snapshot.files]
            .filter(([, file]) => includesTable(file, routingTables))
            .map(([key]) => client.fragments(key)),
        )
      ).flat()
  const aliases: RouteAlias[] = mergedRows(extra, "card_route_alias").map((row) => ({
    namespace: stringValue(row["namespace"]),
    oldKey: stringValue(row["old_key"]),
    targetNamespace: stringValue(row["target_namespace"]),
    targetKey: stringValue(row["target_key"]),
  }))
  const overrides = new Map(
    mergedRows(extra, "route_override").map((row) => [
      stringValue(row["route_key"]),
      stringValue(row["printing_id"]),
    ]),
  )
  const records = (snapshot: LoadedSnapshot): RoutePrinting[] => {
    const catalog = catalogOf(snapshot)
    return catalog.index.cards.flatMap((card) =>
      catalog.index.printingsOf(stringValue(card["id"])).map((row) => ({
        id: stringValue(row["id"]),
        cardId: stringValue(card["id"]),
        cardOwner: stringValue(card["home_set_id"]),
        printingOwner: stringValue(row["home_set_id"]),
        region: stringValue(row["region"]) as Region,
        cardNo: stringValue(row["card_no"]),
        cardNoState: typeof row["card_no_state"] === "string" ? row["card_no_state"] : "official",
        ...(typeof row["int_id"] === "number" ? { intId: row["int_id"] } : {}),
      })),
    )
  }
  const supplement = async (keys: readonly string[]): Promise<void> => {
    const parts = (await Promise.all(keys.map((key) => client.fragments(key)))).flat()
    if (client.snapshot()?.manifestHash !== snapshot.manifestHash)
      throw new CardRouteError("reference", "snapshot changed while opening card")
    const present = new Set(snapshot.bootstrap.map((f) => f.identity))
    snapshot = {
      ...snapshot,
      bootstrap: [...snapshot.bootstrap, ...parts.filter((f) => !present.has(f.identity))],
    }
  }
  if (input.namespace === "provisional") {
    // int_id is deliberately absent from locator triples; read its authoritative printing rows.
    await supplement(
      [...snapshot.files].filter(([, file]) => file["purpose"] === "print").map(([key]) => key),
    )
  }
  if (input.namespace === "official") {
    const keys = new Set([input.key, ...aliases.map((alias) => alias.targetKey)])
    const locate = createLocator(snapshot.files)
    const files = new Set<string>()
    for (const candidate of records(snapshot)) {
      if (
        candidate.intId !== undefined ||
        ![...keys].some((key) => routeFold(key) === routeFold(candidate.cardNo))
      )
        continue
      const key = locate({
        table: "printing",
        owner: homeSetOwner(candidate.printingOwner),
        entityKey: [candidate.id],
        region: candidate.region,
        partition: "bootstrap",
        purpose: "print",
      })
      if (!key) throw new CardRouteError("reference", "printing owner file is missing")
      files.add(key)
    }
    await supplement([...files])
  }
  let printing: RoutePrinting
  try {
    if (input.hint) {
      const hinted = records(snapshot).find((row) => row.id === input.hint?.id)
      if (
        !hinted ||
        (input.namespace === "official"
          ? hinted.cardNo !== input.key
          : String(hinted.intId) !== input.key)
      )
        throw new CardRouteError("reference", "navigation printing does not match route")
      printing = hinted
    } else printing = resolveRoute(input, records(snapshot), aliases, overrides)
  } catch (error) {
    if (
      !(error instanceof CardRouteError) ||
      !["not-found", "needs-update", "reference"].includes(error.kind) ||
      input.hint
    )
      throw error
    const other = snapshot.region === "en" ? "jp" : "en"
    await client.selectRegion?.(other)
    const next = client.snapshot()
    if (!next || next.manifestHash !== snapshot.manifestHash)
      throw new CardRouteError("reference", "route lookup changed snapshot")
    snapshot = next
    if (input.namespace === "provisional")
      await supplement(
        [...snapshot.files].filter(([, file]) => file["purpose"] === "print").map(([key]) => key),
      )
    printing = resolveRoute(input, records(snapshot), aliases, overrides)
  }
  if (
    input.hint &&
    (input.hint.id !== printing.id ||
      input.hint.cardId !== printing.cardId ||
      input.hint.cardOwner !== printing.cardOwner ||
      input.hint.printingOwner !== printing.printingOwner ||
      input.hint.region !== printing.region)
  )
    throw new CardRouteError("reference", "navigation owner does not match card")
  const locate = createLocator(snapshot.files)
  const key = locate({
    table: "printing",
    owner: homeSetOwner(printing.printingOwner),
    entityKey: [printing.id],
    region: printing.region,
    partition: "bootstrap",
    purpose: "print",
  })
  const row = catalogOf(snapshot).index.printing(printing.id)
  if (typeof row?.["int_id"] !== "number") {
    if (!key) throw new CardRouteError("reference", "printing owner file is missing")
    await supplement([key])
  }
  const catalog = catalogOf(snapshot)
  const full = records(snapshot).find((p) => p.id === printing.id)
  if (!full || full.intId === undefined || !catalog.summary(full.id))
    throw new CardRouteError("reference", "card printing reference is missing")
  if (!input.hint && input.namespace === "official" && full.cardNoState !== "official")
    throw new CardRouteError("not-found", "provisional labels do not identify official routes")
  return { printing: full, catalog, snapshot }
}
