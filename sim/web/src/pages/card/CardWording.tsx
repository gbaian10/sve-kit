import { useState } from "react"
import { useTranslation } from "react-i18next"
import { Link as RouterLink } from "react-router"

import { useAnnotatedText } from "../../app/snapshot"
import { CardText } from "../../components/card/CardText"
import { useTextLanguage } from "../../components/card/text-language"
import { type Catalog, type Row, type SnapshotClient, stringValue } from "../../data"
import type { Region } from "../../domain/search"
import { printingGroups } from "./printing-order"
import type { CardLink } from "./RelatedCards"

interface Candidate {
  readonly revisionId: string | null
  readonly printings: readonly { readonly id: string; readonly cardNo: string }[]
}

interface TextProps {
  readonly client: SnapshotClient
  readonly edition: Region
}

function CandidateText({
  candidate,
  selected,
  initiallyOpen,
  client,
  edition,
}: TextProps & {
  readonly candidate: Candidate
  readonly selected: boolean
  readonly initiallyOpen: boolean
}) {
  const { t } = useTranslation()
  const [open, setOpen] = useState(initiallyOpen)
  return (
    <details
      open={open}
      onToggle={(event) => {
        setOpen(event.currentTarget.open)
      }}
      className="rounded-control border border-border px-3 py-2"
    >
      <summary className="cursor-pointer text-14">
        <span className="font-mono">{candidate.printings.map((p) => p.cardNo).join(" / ")}</span>
        {selected && <span className="ml-2 text-text-2">{t("card.wording.shown")}</span>}
      </summary>
      {open &&
        (candidate.revisionId === null ? (
          <p className="mt-2 text-14 text-text-2">{t("card.wording.unavailable")}</p>
        ) : (
          <CardText
            client={client}
            edition={edition}
            owner={{ kind: "face_revision", id: candidate.revisionId }}
            field="effect"
            display="original"
          />
        ))}
    </details>
  )
}

/** The printing that stands for another wording of the same face, for the note under the effect. */
interface OtherWording {
  readonly id: string
  readonly cardNo: string
}

const PLACEHOLDER = "\u0000"

/**
 * One printing's effect: the translation of the text it prints, its own text in the original
 * box, and a note under the effect naming the printings worded differently. A revision without a
 * translation borrows the latest wording's, still beside this printing's own original.
 */
function PrintingWording({
  client,
  edition,
  printingId,
  faceId,
  ownRevisionId,
  translationRevisionId,
  classification,
  others,
  linkOf,
}: TextProps & {
  readonly printingId: string
  readonly faceId: string
  readonly ownRevisionId: string
  readonly translationRevisionId: string
  readonly classification: unknown
  readonly others: readonly OtherWording[]
  readonly linkOf: (printingId: string) => CardLink | undefined
}) {
  const { t } = useTranslation()
  const language = useTextLanguage()
  const own = useAnnotatedText(
    client,
    { kind: "face_revision", id: ownRevisionId },
    "effect",
    language,
    edition,
  )
  const latest = useAnnotatedText(
    client,
    { kind: "face_revision", id: translationRevisionId },
    "effect",
    language,
    edition,
  )
  const printedFace = useAnnotatedText(
    client,
    { kind: "printing_face", id: printingId, face_id: faceId },
    "effect",
    language,
    edition,
  )
  const different = ownRevisionId !== translationRevisionId
  // The official page prints nothing for this printing (an unconfirmed printed text): the latest
  // wording stands alone with the printed-text state line, no original box, no note.
  const blank =
    different &&
    own.value !== null &&
    stringValue(own.value.original.unit["text"]) === "" &&
    own.value.translated === undefined &&
    (own.value.originalSections ?? []).length === 0
  // Only a translation the latest wording actually has is worth borrowing; a language with no
  // translation at all keeps this printing's own text.
  const borrowed =
    different &&
    !blank &&
    own.value !== null &&
    own.value.translated === undefined &&
    stringValue(own.value.original.unit["lang"]) !== language &&
    latest.value?.translated !== undefined
  const text =
    classification === "same_effect"
      ? t("card.wording.printedSameEffect", { printings: PLACEHOLDER })
      : t("card.wording.printedDifferent", { printings: PLACEHOLDER })
  const [before = "", after = ""] = text.split(PLACEHOLDER)
  return (
    <>
      {blank ? (
        <>
          <CardText
            client={client}
            edition={edition}
            owner={{ kind: "face_revision", id: translationRevisionId }}
            field="effect"
          />
          {printedFace.value?.printedState === "unknown" && (
            <p className="mt-2 text-13 text-text-2">{t("card.printedUnconfirmed")}</p>
          )}
          {printedFace.value?.printedState === "derived" && (
            <p className="mt-2 text-13 text-text-2">{t("card.printedDerived")}</p>
          )}
        </>
      ) : borrowed ? (
        <CardText
          client={client}
          edition={edition}
          owner={{ kind: "face_revision", id: translationRevisionId }}
          field="effect"
          printed={{ kind: "face_revision", id: ownRevisionId }}
        />
      ) : (
        <CardText
          client={client}
          edition={edition}
          owner={{ kind: "face_revision", id: ownRevisionId }}
          field="effect"
        />
      )}
      {others.length > 0 && !blank && (
        <p className="mt-2 text-12 text-text-3" data-wording-note>
          {before}
          {others.map((other, index) => {
            const link = linkOf(other.id)
            return (
              <span key={other.id}>
                {index > 0 && t("card.wording.printingJoiner")}
                {link ? (
                  <RouterLink
                    to={link.to}
                    state={link.state}
                    replace
                    preventScrollReset
                    className="font-mono underline decoration-dotted underline-offset-2 hover:text-text-1"
                  >
                    {other.cardNo}
                  </RouterLink>
                ) : (
                  <span className="font-mono">{other.cardNo}</span>
                )}
              </span>
            )
          })}
          {after}
        </p>
      )}
    </>
  )
}

export function CardWording({
  client,
  edition,
  catalog,
  printingId,
  wording,
  revision,
  linkOf,
}: TextProps & {
  readonly catalog: Catalog
  readonly printingId: string
  readonly wording: Row | undefined
  readonly revision: Row | undefined
  /** Where a printing chip of this card leads, for the note's card numbers. */
  readonly linkOf: (printingId: string) => CardLink | undefined
}) {
  const { t } = useTranslation()
  // A display without an adopted current: say so, whichever branch shows the printing's own text.
  const provisionalNote = (wording?.["display"] as Row | undefined)?.["basis"] ===
    "latest_known_release" && (
    <p role="note" className="mt-3 text-12 text-text-3">
      {t("card.wording.provisional")}
    </p>
  )
  const grouped = new Map<string | null, Candidate["printings"][number][]>()
  for (const row of (wording?.["candidates"] ?? []) as Row[]) {
    const id = stringValue(row["printing_id"])
    const printing = catalog.index.printing(id)
    if (!printing) continue
    const revisionId = typeof row["revision_id"] === "string" ? row["revision_id"] : null
    const printings = grouped.get(revisionId) ?? []
    if (!printings.some((p) => p.id === id))
      printings.push({ id, cardNo: stringValue(printing["card_no"]) })
    grouped.set(revisionId, printings)
  }
  const candidates = [...grouped].map(([revisionId, printings]) => ({ revisionId, printings }))
  const own = candidates.filter((candidate) => candidate.printings.some((p) => p.id === printingId))
  const basisRevision = catalog.index.displayRevision(
    stringValue(revision?.["face_id"] ?? ""),
    edition,
  )
  const displayId = basisRevision
    ? stringValue(basisRevision["id"])
    : revision
      ? stringValue(revision["id"])
      : undefined
  if (displayId && !candidates.some((candidate) => candidate.revisionId === displayId))
    candidates.unshift({ revisionId: displayId, printings: [] })
  const display = wording?.["display"] as Row | undefined
  const ownId = own.length === 1 ? own[0]?.revisionId : null
  // One printing stands for each other wording: the latest wording's basis printing, else the
  // first of that wording's printings in chip order.
  const chipOrder = new Map(
    printingGroups(
      candidates.flatMap((candidate) =>
        candidate.printings.flatMap((p) => {
          const row = catalog.index.printing(p.id)
          return row ? [row] : []
        }),
      ),
    )
      .flatMap((group) => group.printings)
      .map((row, index) => [stringValue(row["id"]), index] as const),
  )
  const others: OtherWording[] = candidates.flatMap((candidate) => {
    if (candidate.revisionId === null || candidate.revisionId === ownId) return []
    const basis =
      candidate.revisionId === displayId
        ? candidate.printings.find((p) => p.id === wording?.["basis_printing_id"])
        : undefined
    const first = [...candidate.printings].sort(
      (a, b) => (chipOrder.get(a.id) ?? 0) - (chipOrder.get(b.id) ?? 0),
    )[0]
    const pick = basis ?? first
    return pick ? [pick] : []
  })
  let body
  if (
    ownId &&
    displayId &&
    (wording?.["classification"] === "same_effect" || display?.["basis"] === "current")
  )
    body = (
      <PrintingWording
        key={printingId}
        client={client}
        edition={edition}
        printingId={printingId}
        faceId={stringValue(revision?.["face_id"] ?? "")}
        ownRevisionId={ownId}
        translationRevisionId={displayId}
        classification={wording?.["classification"]}
        others={others}
        linkOf={linkOf}
      />
    )
  else if (!wording)
    body = displayId ? (
      <CardText
        client={client}
        edition={edition}
        owner={{ kind: "face_revision", id: displayId }}
        field="effect"
      />
    ) : (
      <p className="mt-3 text-14 text-text-2">{t("card.wording.unavailable")}</p>
    )
  else if (ownId)
    body = (
      <CardText
        client={client}
        edition={edition}
        owner={{ kind: "face_revision", id: ownId }}
        field="effect"
        display="original"
      />
    )
  else
    body = (
      <section className="mt-4 space-y-2" aria-label={t("card.wording.different")}>
        <h3 className="text-14 font-semibold">{t("card.wording.different")}</h3>
        {candidates.length === 0 && <p>{t("card.wording.unavailable")}</p>}
        {candidates.map((candidate) => {
          const selected =
            candidate.revisionId === displayId ||
            (displayId === undefined && candidates.length === 1)
          return (
            <CandidateText
              key={`${printingId}/${candidate.revisionId ?? "unknown"}`}
              client={client}
              edition={edition}
              candidate={candidate}
              selected={selected}
              // With no wording displayed yet, open the one printed on this printing.
              initiallyOpen={
                selected ||
                (displayId === undefined && candidate.printings.some((p) => p.id === printingId))
              }
            />
          )
        })}
      </section>
    )
  return (
    <>
      {provisionalNote}
      {body}
    </>
  )
}
