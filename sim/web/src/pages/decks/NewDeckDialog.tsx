import { useState } from "react"
import { useTranslation } from "react-i18next"

import { useDialogHistory } from "../../app/dialog-history"
import { CLASS_ICON } from "../../components/card/classIcons"
import { Button } from "../../components/ui/Button"
import { cn } from "../../components/ui/cn"
import { Dialog } from "../../components/ui/Dialog"
import { Segmented } from "../../components/ui/Segmented"
import { classStyle } from "../../domain/classes"
import type { DeckBasis } from "../../domain/deck/types"
import type { Region } from "../../domain/search"
import type { ClassOption } from "./deck-labels"
import type { TitleOption } from "./deck-titles"

export interface NewDeckInput {
  readonly name: string
  readonly edition: Region
  readonly basis: DeckBasis
}

/**
 * One step: name, edition and the construction basis (a class, neutral, or a title). The edition
 * is the card edition preference, as on the cards page, so the title list always belongs to it.
 */
export function NewDeckDialog({
  open,
  onClose,
  edition,
  onEditionChange,
  defaultName,
  classOptions,
  titles,
  titleName,
  onCreate,
}: {
  readonly open: boolean
  readonly onClose: () => void
  readonly edition: Region
  readonly onEditionChange: (edition: Region) => void
  /** Used when the name is left empty. */
  readonly defaultName: string
  readonly classOptions: readonly ClassOption[] | undefined
  /** The edition's titles; undefined while its cards load. */
  readonly titles: readonly TitleOption[] | undefined
  readonly titleName: (code: string) => string
  readonly onCreate: (input: NewDeckInput) => void
}) {
  const { t } = useTranslation()
  const [name, setName] = useState("")
  const [kind, setKind] = useState<DeckBasis["kind"]>("class")
  const [classCode, setClassCode] = useState<string | null>(null)
  const [titleCode, setTitleCode] = useState<string | null>(null)
  // A choice the current options no longer offer (another edition's title) falls back to the first.
  const chosenClass = classOptions?.some((option) => option.code === classCode)
    ? (classCode ?? undefined)
    : classOptions?.[0]?.code
  const chosenTitle = titles?.some((title) => title.code === titleCode)
    ? (titleCode ?? undefined)
    : titles?.[0]?.code
  const basis: DeckBasis | undefined =
    kind === "class"
      ? chosenClass === undefined
        ? undefined
        : { kind, classCode: chosenClass }
      : chosenTitle === undefined
        ? undefined
        : { kind, titleCode: chosenTitle }
  const close = useDialogHistory("new-deck", open, () => {
    setName("")
    setKind("class")
    setClassCode(null)
    setTitleCode(null)
    onClose()
  })
  return (
    <Dialog
      open={open}
      onClose={close}
      title={t("decks.newDeck")}
      variant="sheet"
      footer={
        <div className="flex gap-2">
          <Button
            variant="primary"
            disabled={basis === undefined}
            onClick={() => {
              if (!basis) return
              onCreate({ name: name.trim() || defaultName, edition, basis })
              close()
            }}
          >
            {t("decks.create")}
          </Button>
          <Button variant="ghost" onClick={close}>
            {t("settings.resetCancel")}
          </Button>
        </div>
      }
    >
      <div className="flex flex-col gap-4 pb-2">
        <label className="flex flex-col gap-1 text-13 text-text-2">
          {t("decks.nameLabel")}
          <input
            value={name}
            onChange={(event) => {
              setName(event.target.value)
            }}
            placeholder={defaultName}
            className="h-11 rounded-button border border-border bg-surface-1 px-3 text-15 text-text-1"
          />
        </label>
        <div className="flex flex-col gap-1 text-13 text-text-2">
          <span>{t("account.cardEdition")}</span>
          <Segmented
            className="self-start"
            label={t("account.cardEdition")}
            value={edition}
            options={[
              { value: "jp", label: t("options.edition.jp") },
              { value: "en", label: t("options.edition.en") },
            ]}
            onChange={onEditionChange}
          />
          <span className="text-11 text-text-3">{t("decks.editionHint")}</span>
        </div>
        <div className="flex flex-col gap-2 text-13 text-text-2">
          <span>{t("decks.basis.label")}</span>
          <Segmented
            className="self-start"
            label={t("decks.basis.label")}
            value={kind}
            options={[
              { value: "class", label: t("decks.basis.class") },
              { value: "title", label: t("decks.basis.title") },
            ]}
            onChange={setKind}
          />
          {kind === "class" ? (
            <ul className="grid grid-cols-2 gap-2 md:grid-cols-3" aria-label={t("search.classes")}>
              {(classOptions ?? []).map((option) => {
                const style = classStyle(option.code)
                const selected = option.code === chosenClass
                return (
                  <li key={option.code}>
                    <button
                      type="button"
                      aria-pressed={selected}
                      data-class={style}
                      onClick={() => {
                        setClassCode(option.code)
                      }}
                      className={cn(
                        "flex h-11 w-full items-center gap-2 rounded-button border px-2 text-13 font-semibold",
                        selected
                          ? "border-class-current bg-surface-3 text-text-1 ring-1 ring-class-current"
                          : "border-border bg-surface-1 text-text-2 hover:bg-surface-2",
                      )}
                    >
                      <img src={CLASS_ICON[style]} alt="" className="size-5" />
                      <span className="truncate">{option.label}</span>
                    </button>
                  </li>
                )
              })}
            </ul>
          ) : (
            <>
              <ul
                className="grid grid-cols-1 gap-2 md:grid-cols-2"
                aria-label={t("decks.basis.title")}
              >
                {(titles ?? []).map((title) => {
                  const selected = title.code === chosenTitle
                  return (
                    <li key={title.code}>
                      <button
                        type="button"
                        aria-pressed={selected}
                        onClick={() => {
                          setTitleCode(title.code)
                        }}
                        className={cn(
                          "flex min-h-11 w-full items-center justify-between gap-2 rounded-button border px-3 text-start text-13 font-semibold",
                          selected
                            ? "border-text-1 bg-surface-3 text-text-1 ring-1 ring-text-1"
                            : "border-border bg-surface-1 text-text-2 hover:bg-surface-2",
                        )}
                      >
                        <span className="min-w-0">{titleName(title.code)}</span>
                        <span className="shrink-0 text-11 text-text-3 tabular-nums">
                          {title.count}
                        </span>
                      </button>
                    </li>
                  )
                })}
              </ul>
              <span className="text-11 text-text-3">{t("decks.basis.titleHint")}</span>
            </>
          )}
          {(kind === "class" ? classOptions : titles) === undefined && (
            <span role="status" className="text-12 text-text-3">
              {t("results.dependenciesPending")}
            </span>
          )}
        </div>
      </div>
    </Dialog>
  )
}
