import { useState } from "react"
import { useTranslation } from "react-i18next"
import { Link } from "react-router"

import { useAnnotatedText } from "../../app/snapshot"
import { CardText } from "../../components/card/CardText"
import { useTextLanguage } from "../../components/card/text-language"
import { TextPurposeStatus } from "../../components/card/TextPurposeStatus"
import { type Catalog, type Row, type SnapshotClient, stringValue } from "../../data"
import type { Region } from "../../domain/search"
import { usePrefs } from "../../settings"

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
          />
        ))}
    </details>
  )
}

function PrintingWording({
  client,
  edition,
  ownRevisionId,
  translationRevisionId,
  basisPrinting,
  entry,
}: TextProps & {
  readonly ownRevisionId: string
  readonly translationRevisionId: string
  readonly basisPrinting: Candidate["printings"][number] | undefined
  readonly entry: unknown
}) {
  const { t } = useTranslation()
  const prefs = usePrefs()
  const language = useTextLanguage()
  const translation = useAnnotatedText(
    client,
    { kind: "face_revision", id: translationRevisionId },
    "effect",
    language,
    edition,
  )
  const different = ownRevisionId !== translationRevisionId
  const showTranslation =
    prefs.effectLanguage !== "original" && translation.value?.translated !== undefined
  return (
    <>
      {different && prefs.effectLanguage !== "original" && (
        <>
          {translation.readiness.basic === "pending" && (
            <p role="status" className="mt-3 text-13 text-text-2">
              {t("card.annotationPending")}
            </p>
          )}
          <TextPurposeStatus readiness={translation.readiness} />
        </>
      )}
      {different && showTranslation && (
        <>
          <p className="mt-3 text-13 text-text-2">
            {t("card.wording.translationBasis")}{" "}
            {basisPrinting && (
              <Link
                to={`/cards/${encodeURIComponent(basisPrinting.cardNo)}`}
                state={entry}
                replace
                className="text-accent-text underline underline-offset-2"
              >
                {basisPrinting.cardNo}
              </Link>
            )}
          </p>
          <CardText
            client={client}
            edition={edition}
            owner={{ kind: "face_revision", id: translationRevisionId }}
            field="effect"
            display="translated"
          />
        </>
      )}
      <CardText
        client={client}
        edition={edition}
        owner={{ kind: "face_revision", id: ownRevisionId }}
        field="effect"
        display={different ? "original" : undefined}
        label={different ? t("card.wording.printingOriginal") : undefined}
      />
      {different &&
        prefs.effectLanguage !== "original" &&
        (translation.readiness.basic === "ready" || translation.readiness.basic === "none") &&
        !showTranslation &&
        language !== (edition === "jp" ? "ja" : "en") && (
          <p className="mt-2 text-13 text-text-2">{t("card.translationMissing")}</p>
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
  entry,
}: TextProps & {
  readonly catalog: Catalog
  readonly printingId: string
  readonly wording: Row | undefined
  readonly revision: Row | undefined
  readonly entry: unknown
}) {
  const { t } = useTranslation()
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
  const displayId = revision ? stringValue(revision["id"]) : undefined
  if (displayId && !candidates.some((candidate) => candidate.revisionId === displayId))
    candidates.unshift({ revisionId: displayId, printings: [] })
  const display = wording?.["display"] as Row | undefined
  const undated = (wording?.["undated_printing_ids"] ?? []) as string[]
  const basisPrinting = candidates
    .find((candidate) => candidate.revisionId === displayId)
    ?.printings.find((p) => !undated.includes(p.id))
  const ownId = own.length === 1 ? own[0]?.revisionId : null
  if (ownId && displayId && display?.["basis"] === "latest_known_release" && basisPrinting) {
    return (
      <PrintingWording
        key={printingId}
        client={client}
        edition={edition}
        ownRevisionId={ownId}
        translationRevisionId={displayId}
        basisPrinting={basisPrinting}
        entry={entry}
      />
    )
  }
  if (!wording)
    return displayId ? (
      <CardText
        client={client}
        edition={edition}
        owner={{ kind: "face_revision", id: displayId }}
        field="effect"
      />
    ) : (
      <p className="mt-3 text-14 text-text-2">{t("card.wording.unavailable")}</p>
    )
  return (
    <section className="mt-4 space-y-2" aria-label={t("card.wording.different")}>
      <h3 className="text-14 font-semibold">{t("card.wording.different")}</h3>
      {candidates.length === 0 && <p>{t("card.wording.unavailable")}</p>}
      {candidates.map((candidate) => {
        const selected =
          candidate.revisionId === displayId || (displayId === undefined && candidates.length === 1)
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
}
