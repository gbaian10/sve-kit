import { type ReactNode, useState } from "react"
import { useTranslation } from "react-i18next"

import { useAnnotatedText } from "../../app/snapshot"
import { CardText } from "../../components/card/CardText"
import { useTextLanguage } from "../../components/card/text-language"
import { Segmented } from "../../components/ui/Segmented"
import type { SnapshotClient } from "../../data"
import type { Region } from "../../domain/search"

export function CardTextPanel({
  client,
  edition,
  printingId,
  faceId,
  children,
}: {
  readonly client: SnapshotClient
  readonly edition: Region
  readonly printingId: string
  readonly faceId: string
  readonly children: ReactNode
}) {
  const { t } = useTranslation()
  const language = useTextLanguage()
  const [mode, setMode] = useState<"current" | "printed">("current")
  const owner = { kind: "printing_face", id: printingId, face_id: faceId }
  const printedName = useAnnotatedText(client, owner, "name", language, edition)
  const printedEffect = useAnnotatedText(client, owner, "effect", language, edition)
  const flavor = useAnnotatedText(client, owner, "flavor", language, edition)
  const hasPrintedText = printedName.value !== null || printedEffect.value !== null
  const showPrinted = mode === "printed" && hasPrintedText
  return (
    <>
      {showPrinted ? (
        <>
          {printedName.value && (
            <CardText client={client} edition={edition} owner={owner} field="name" />
          )}
          {printedEffect.value && (
            <CardText client={client} edition={edition} owner={owner} field="effect" />
          )}
        </>
      ) : (
        children
      )}
      {/* Flavor belongs to this printing alone, so both views show it. */}
      {flavor.value && <CardText client={client} edition={edition} owner={owner} field="flavor" />}
      {hasPrintedText && (
        <Segmented
          className="mt-4"
          size="sm"
          label={t("card.textMode")}
          options={[
            { value: "current", label: t("card.currentText") },
            { value: "printed", label: t("card.printedText") },
          ]}
          value={showPrinted ? "printed" : "current"}
          onChange={setMode}
        />
      )}
    </>
  )
}
