import { useTranslation } from "react-i18next"

import type { AnnotatedTextState } from "../../app/snapshot"

export function TextPurposeStatus({
  readiness,
}: {
  readonly readiness: AnnotatedTextState["readiness"]
}) {
  const { t } = useTranslation()
  if (readiness.basic === "error")
    return (
      <p role="alert">
        {readiness.error?.kind === "validation" ? t("card.textFailed") : t("card.textLoadFailed")}
      </p>
    )
  if (readiness.basic !== "ready") return null
  return (
    <>
      {readiness.source === "pending" && (
        <p role="status" className="text-13">
          {t("card.sourcePending")}
        </p>
      )}
      {readiness.annotations === "pending" && (
        <p role="status" className="text-13">
          {t("card.annotationsPending")}
        </p>
      )}
      {readiness.source === "error" && (
        <p role="alert" className="text-13">
          {readiness.error?.kind === "validation" ? t("card.textFailed") : t("card.sourceFailed")}
        </p>
      )}
      {readiness.annotations === "error" && (
        <p role="alert" className="text-13">
          {readiness.error?.kind === "validation"
            ? t("card.textFailed")
            : t("card.annotationsFailed")}
        </p>
      )}
    </>
  )
}
