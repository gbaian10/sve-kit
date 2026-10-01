import { useTranslation } from "react-i18next"
import { useParams } from "react-router"

import { useCatalog } from "../../app/snapshot"
import type { Row } from "../../data"
import { usePrefs } from "../../settings"
import { PageTitle } from "../PageTitle"

export function CardPage() {
  const { t } = useTranslation()
  const { cardNo, intId } = useParams()
  const { catalog } = useCatalog()
  const { cardEdition } = usePrefs()
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
      {faces.map((face) => {
        const faceId = face["id"] as string
        const wording = catalog?.index.wording(faceId, region)
        if (!wording) return null
        const revision = catalog?.index.displayRevision(faceId, region)
        const name = revision
          ? catalog?.index.textUnit(revision["name_unit_id"] as string)?.["text"]
          : undefined
        const undated = (wording["undated_printing_ids"] as string[]).map(
          (id) => (catalog?.index.printing(id)?.["card_no"] as string | undefined) ?? id,
        )
        return (
          <section key={faceId} className="mt-4 space-y-2" aria-label={t("card.wordingPending")}>
            <h2 className="text-14 font-semibold">{t("card.wordingPending")}</h2>
            {typeof name === "string" && <p>{name}</p>}
            {(wording["display"] as Row)["basis"] === "latest_known_release" && revision && (
              <p>{t("card.provisionalWording")}</p>
            )}
            {!revision && <p>{t("card.candidatesUnselected")}</p>}
            {undated.length > 0 && (
              <div className="flex flex-wrap gap-x-2 gap-y-1">
                <span>{t("card.undatedPrintings")}</span>
                <ul className="flex flex-wrap gap-x-2 gap-y-1">
                  {undated.map((cardNo) => (
                    <li key={cardNo} className="font-mono text-14 whitespace-nowrap">
                      {cardNo}
                    </li>
                  ))}
                </ul>
              </div>
            )}
          </section>
        )
      })}
    </>
  )
}
