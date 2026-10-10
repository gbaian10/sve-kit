import { useState } from "react"
import { useTranslation } from "react-i18next"

import {
  type AnnotatedText as TextValue,
  arrayValue,
  canonicalText,
  type JsonObject,
  type SelectedText,
  stringValue,
  textRuns,
} from "../../data"
import { Link } from "../ui/Link"

export function AnnotatedText({
  value,
  context,
  emphasis,
}: {
  readonly value: TextValue
  readonly context: SelectedText
  readonly emphasis: boolean
}) {
  const { t } = useTranslation()
  const [reference, setReference] = useState<JsonObject | null>(null)
  const concept =
    reference &&
    context.concepts.find(
      (row) => row["id"] === reference[reference["kind"] === "card_name" ? "term_id" : "key"],
    )
  const explanations = concept
    ? arrayValue(concept["explanations"])
        .map((ref) =>
          context.explanations.find(
            (value) => canonicalText(value.reference) === canonicalText(ref),
          ),
        )
        .filter((value) => value !== undefined)
    : []
  return (
    <div>
      <p lang={stringValue(value.unit["lang"])} className="whitespace-pre-wrap">
        {textRuns(value).map((run, index) => {
          const content = emphasis && run.bold === true ? <strong>{run.text}</strong> : run.text
          return run.reference ? (
            <button
              key={index}
              type="button"
              className="inline cursor-pointer text-inherit underline decoration-dotted underline-offset-4"
              title={t("card.annotationReference")}
              onClick={() => {
                setReference(run.reference ?? null)
              }}
            >
              {content}
            </button>
          ) : (
            <span key={index}>{content}</span>
          )
        })}
      </p>
      {reference && (
        <aside
          aria-label={t("card.annotationReference")}
          className="mt-2 rounded-block bg-surface-2 p-3"
        >
          {explanations.map((value) => (
            <div key={canonicalText(value.reference)}>
              {value.reference["kind"] === "ruling_revision" && (
                <>
                  <p>
                    {t("card.projectRuling")} ·{" "}
                    {value.row["strength"] === "official"
                      ? t("card.rulingOfficial")
                      : value.row["strength"] === "generalized"
                        ? t("card.rulingGeneralized")
                        : value.row["strength"] === "inferred"
                          ? t("card.rulingInferred")
                          : t("card.rulingUndecided")}
                  </p>
                  {value.row["review_state"] === "needs_review" && (
                    <p>{t("card.rulingNeedsReview")}</p>
                  )}
                  {value.row["review_state"] === "superseded" && (
                    <p>{t("card.rulingSuperseded")}</p>
                  )}
                </>
              )}
              <p lang={stringValue(value.unit["lang"])} className="whitespace-pre-wrap">
                {stringValue(value.unit["text"])}
              </p>
            </div>
          ))}
          {explanations.length === 0 && <p>{t("card.noExplanation")}</p>}
          {concept &&
            context.cards
              .filter((card) => arrayValue(concept["card_ids"]).includes(card.cardId))
              .map((card) => (
                <Link key={card.intId} to={`/p/${String(card.intId)}`}>
                  {card.cardNo}
                </Link>
              ))}
          <button
            type="button"
            onClick={() => {
              setReference(null)
            }}
          >
            {t("card.closeReference")}
          </button>
        </aside>
      )}
    </div>
  )
}
