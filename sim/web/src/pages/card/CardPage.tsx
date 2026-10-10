import { useMemo, useState } from "react"
import { useTranslation } from "react-i18next"
import { useParams } from "react-router"

import { useCatalog } from "../../app/snapshot"
import { CardText } from "../../components/card/CardText"
import { Segmented } from "../../components/ui/Segmented"
import { createAnnotatedTextResolver, type Row } from "../../data"
import { usePrefs } from "../../settings"
import { PageTitle } from "../PageTitle"

function cardNoColumns(cardNo: string): number {
  // Symbols such as Ⓢ can use wide fallback glyphs despite the monospaced Latin digits.
  return Array.from(cardNo).reduce(
    (width, character) => width + (character.charCodeAt(0) <= 0x7f ? 1 : 2),
    0,
  )
}

const CURRENT_FIELDS = ["name", "effect"] as const
const PRINTED_FIELDS = ["name", "effect", "flavor"] as const

export function CardPage() {
  const { t } = useTranslation()
  const { cardNo, intId } = useParams()
  const { catalog, client } = useCatalog()
  const [textMode, setTextMode] = useState<"current" | "printed">("current")
  const { cardEdition } = usePrefs()
  const snapshot = client.snapshot()
  const pageKey = cardNo ?? intId ?? ""
  const resolver = useMemo(
    () => (snapshot && pageKey ? createAnnotatedTextResolver(client) : null),
    [client, snapshot, pageKey],
  )
  const printing = cardNo
    ? (catalog?.index.printingByCardNo(cardEdition, cardNo) ??
      catalog?.index.printingByCardNo(cardEdition === "jp" ? "en" : "jp", cardNo))
    : catalog?.index.cards
        .flatMap((card) => catalog.index.printingsOf(card["id"] as string))
        .find((row) => typeof row["int_id"] === "number" && String(row["int_id"]) === intId)
  const region = (printing?.["region"] as string | undefined) ?? cardEdition
  const faces = printing ? (catalog?.index.facesOf(printing["card_id"] as string) ?? []) : []
  return (
    <>
      <PageTitle>
        {t("pages.card")}
        <span className="ml-2 font-mono text-16 whitespace-nowrap text-text-2">
          {cardNo ?? intId}
        </span>
      </PageTitle>
      <Segmented
        label={t("card.textMode")}
        options={[
          { value: "current", label: t("card.currentText") },
          { value: "printed", label: t("card.printedText") },
        ]}
        value={textMode}
        onChange={setTextMode}
      />
      {faces.map((face) => {
        const faceId = face["id"] as string
        const wording = catalog?.index.wording(faceId, region)
        const revision = catalog?.index.displayRevision(faceId, region)
        const name = revision
          ? catalog?.index.textUnit(revision["name_unit_id"] as string)?.["text"]
          : undefined
        const undated = ((wording?.["undated_printing_ids"] ?? []) as string[]).map(
          (id) => (catalog?.index.printing(id)?.["card_no"] as string | undefined) ?? id,
        )
        return (
          <div key={faceId}>
            {printing &&
              (textMode === "printed" || revision) &&
              (textMode === "printed" ? PRINTED_FIELDS : CURRENT_FIELDS).map((field) => (
                <CardText
                  key={field}
                  client={client}
                  sharedResolver={resolver}
                  owner={
                    textMode === "printed"
                      ? { kind: "printing_face", id: printing["id"] as string, face_id: faceId }
                      : { kind: "face_revision", id: revision?.["id"] as string }
                  }
                  field={field}
                />
              ))}
            {wording && (
              <section className="mt-4 space-y-2" aria-label={t("card.wordingPending")}>
                <h2 className="text-14 font-semibold">{t("card.wordingPending")}</h2>
                {typeof name === "string" && <p>{name}</p>}
                {(wording["display"] as Row)["basis"] === "latest_known_release" && revision && (
                  <p>{t("card.provisionalWording")}</p>
                )}
                {!revision && <p>{t("card.candidatesUnselected")}</p>}
                {undated.length > 0 && (
                  <div className="space-y-1">
                    <span>{t("card.undatedPrintings")}</span>
                    <ul
                      className="grid gap-x-3 gap-y-1 font-mono text-14"
                      style={{
                        gridTemplateColumns: `repeat(auto-fill, ${String(Math.max(...undated.map(cardNoColumns)))}ch)`,
                      }}
                    >
                      {undated.map((cardNo) => (
                        <li key={cardNo} className="text-left whitespace-nowrap">
                          {cardNo}
                        </li>
                      ))}
                    </ul>
                  </div>
                )}
              </section>
            )}
          </div>
        )
      })}
    </>
  )
}
