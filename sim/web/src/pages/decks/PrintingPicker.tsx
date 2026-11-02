import { Check } from "lucide-react"
import { useTranslation } from "react-i18next"

import { useDialogHistory } from "../../app/dialog-history"
import { CardImage } from "../../components/card/CardImage"
import { cn } from "../../components/ui/cn"
import { Dialog } from "../../components/ui/Dialog"
import type { ImageIndex } from "../../data"
import type { DeckSection } from "../../domain/deck/types"
import { displayName } from "../../domain/nameDisplay"
import { useDeckEditor } from "./deck-editor"

export interface PrintingTarget {
  readonly cardId: string
  readonly section: DeckSection | "leader"
}

/** Which printing of a card the owner plays (alt art, reprint); the deck keeps it per card. */
export function PrintingPicker({
  target,
  images,
  onClose,
}: {
  readonly target: PrintingTarget | null
  readonly images: ImageIndex | undefined
  readonly onClose: () => void
}) {
  const { t } = useTranslation()
  const editor = useDeckEditor()
  const close = useDialogHistory("deck-printing", target !== null, onClose)
  const facts = target ? editor.facts?.factsOf(target.cardId) : undefined
  const current =
    target === null
      ? undefined
      : target.section === "leader"
        ? editor.deck.leader?.printingId
        : editor.deck[target.section].find((entry) => entry.cardId === target.cardId)?.printingId
  const chosen = current ?? facts?.defaultPrintingId
  return (
    <Dialog
      open={target !== null}
      onClose={close}
      title={t("decks.pickPrintingTitle", { name: facts?.name ?? "" })}
      variant="sheet"
    >
      <p className="mb-3 text-13 text-text-2">{t("decks.pickPrintingHint")}</p>
      <ul className="grid grid-cols-3 gap-3 pb-2 md:grid-cols-4">
        {(facts?.printings ?? []).map((printing) => {
          const summary = printing.summary
          const selected = printing.id === chosen
          const rarity = summary?.rarity
          return (
            <li key={printing.id}>
              <button
                type="button"
                aria-pressed={selected}
                onClick={() => {
                  if (!target) return
                  editor.setPrinting(
                    target.section,
                    target.cardId,
                    printing.id === facts?.defaultPrintingId ? undefined : printing.id,
                  )
                  close()
                }}
                className={cn(
                  "flex w-full flex-col gap-1 rounded-card text-start",
                  selected && "ring-2 ring-accent ring-offset-2 ring-offset-surface-1",
                )}
              >
                {summary ? (
                  <CardImage
                    summary={summary}
                    name={displayName(summary.name, editor.uiLanguage, editor.nameDisplay).primary}
                    images={images}
                    alt="redundant"
                    size="large"
                    className="w-full"
                  />
                ) : (
                  <span className="block aspect-[63/88] w-full rounded-card bg-surface-2" />
                )}
                <span className="flex items-center justify-between gap-1 text-11 text-text-2 tabular-nums">
                  <span className="truncate">{printing.cardNo}</span>
                  <span className="truncate text-text-3">
                    {rarity?.translations[editor.textLang] ??
                      rarity?.original.text ??
                      printing.rarityCode ??
                      ""}
                  </span>
                  {selected && (
                    <Check className="size-3.5 shrink-0 text-accent-text" aria-hidden="true" />
                  )}
                </span>
                {printing.id === facts?.defaultPrintingId && (
                  <span className="text-11 text-text-3">{t("decks.defaultPrinting")}</span>
                )}
              </button>
            </li>
          )
        })}
      </ul>
    </Dialog>
  )
}
