import { Copy, Layers, Pencil, Plus, Trash2 } from "lucide-react"
import { useMemo, useState } from "react"
import { useTranslation } from "react-i18next"
import { useLocation, useNavigate } from "react-router"

import { deckPath } from "../../app/deck-routes"
import { useDialogHistory } from "../../app/dialog-history"
import { useRouteEdition } from "../../app/snapshot"
import { CLASS_ICON } from "../../components/card/classIcons"
import { Badge } from "../../components/ui/Badge"
import { Button } from "../../components/ui/Button"
import { Dialog } from "../../components/ui/Dialog"
import { Link } from "../../components/ui/Link"
import { Notice } from "../../components/ui/Notice"
import { useToast } from "../../components/ui/toast-context"
import { classStyle } from "../../domain/classes"
import { createDeck, duplicateDeck, renameDeck, totalCount } from "../../domain/deck/model"
import type { Deck } from "../../domain/deck/types"
import { currentUiLanguage } from "../../i18n"
import { decksStore, useDecks, usePrefs } from "../../settings"
import { PageTitle } from "../PageTitle"
import { useDeckLabels } from "./deck-labels"
import { NewDeckDialog } from "./NewDeckDialog"

export function DecksPage() {
  const { t } = useTranslation()
  const state = useDecks()
  const edition = useRouteEdition()
  const prefs = usePrefs()
  const uiLanguage = currentUiLanguage(prefs.uiLanguage)
  const labels = useDeckLabels()
  const toast = useToast()
  const navigate = useNavigate()
  const location = useLocation()
  const [newOpen, setNewOpen] = useState(false)
  const [deleting, setDeleting] = useState<Deck | null>(null)
  const [renaming, setRenaming] = useState<Deck | null>(null)
  const [renameValue, setRenameValue] = useState("")
  const dateFormat = useMemo(
    () => new Intl.DateTimeFormat(uiLanguage, { dateStyle: "medium", timeStyle: "short" }),
    [uiLanguage],
  )
  const closeDelete = useDialogHistory("delete-deck", deleting !== null, () => {
    setDeleting(null)
  })
  const closeRename = useDialogHistory("rename-deck", renaming !== null, () => {
    setRenaming(null)
  })
  const decks = state.status === "ready" ? state.decks : []
  // Storage failures are rare (quota, a blocked database); say so instead of dropping the change.
  const persist = (work: Promise<void>) => {
    work.catch(() => {
      toast.show({ message: t("decks.saveFailed") })
    })
  }
  const newButton = (
    <Button
      variant="primary"
      disabled={state.status !== "ready"}
      onClick={() => {
        setNewOpen(true)
      }}
    >
      <Plus className="size-4" aria-hidden="true" />
      {t("decks.newDeck")}
    </Button>
  )
  return (
    <div className="flex flex-col gap-4 pb-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <PageTitle>{t("decks.pageTitle")}</PageTitle>
        {newButton}
      </div>
      <p className="text-13 text-text-2">{t("decks.listHint")}</p>
      {state.status === "unavailable" && (
        <Notice tone="warning" role="alert">
          {t("decks.unavailable")}
        </Notice>
      )}
      {state.status === "loading" && (
        <p role="status" className="text-13 text-text-2">
          {t("decks.loading")}
        </p>
      )}
      {state.status === "ready" && decks.length === 0 && (
        <div className="flex flex-col items-center gap-3 rounded-block border border-dashed border-border-strong px-4 py-12 text-center">
          <Layers className="size-8 text-text-3" aria-hidden="true" />
          <p className="text-16 font-semibold">{t("decks.emptyTitle")}</p>
          <p className="max-w-md text-13 text-text-3">{t("decks.emptyHint")}</p>
          {newButton}
        </div>
      )}
      {decks.length > 0 && (
        <ul aria-label={t("decks.pageTitle")} className="grid gap-3 md:grid-cols-2 xl:grid-cols-3">
          {decks.map((deck) => {
            const style = classStyle(deck.basis.kind === "class" ? deck.basis.classCode : null)
            return (
              <li
                key={deck.id}
                data-class={style}
                className="flex gap-3 rounded-block border border-l-4 border-border border-l-class-current bg-surface-1 p-3"
              >
                <span className="flex aspect-[63/88] w-14 shrink-0 items-center justify-center rounded-card border border-dashed border-border-strong bg-surface-2">
                  <img src={CLASS_ICON[style]} alt="" className="size-7 opacity-70" />
                </span>
                <div className="flex min-w-0 flex-1 flex-col gap-1">
                  <h2 className="truncate text-16 font-bold">
                    <Link to={deckPath(deck)} className="text-text-1 no-underline hover:underline">
                      {deck.name}
                    </Link>
                  </h2>
                  <p className="flex flex-wrap items-center gap-1.5 text-12 text-text-2">
                    <span className="font-semibold text-class-current">
                      {labels.basisLabel(deck.basis)}
                    </span>
                    <Badge>{t(`options.edition.${deck.edition}`)}</Badge>
                  </p>
                  <p className="text-12 text-text-2 tabular-nums">
                    {t("decks.counts", {
                      main: totalCount(deck.main),
                      evolve: totalCount(deck.evolve),
                    })}
                  </p>
                  <p className="text-11 text-text-3">
                    {t("decks.updatedAt", { date: dateFormat.format(new Date(deck.updatedAt)) })}
                  </p>
                  <div className="mt-1 flex flex-wrap gap-1">
                    <Button
                      size="sm"
                      aria-label={t("decks.duplicateNamed", { name: deck.name })}
                      onClick={() => {
                        persist(
                          decksStore.save(
                            duplicateDeck(
                              deck,
                              t("decks.copyName", { name: deck.name }),
                              new Date(),
                            ),
                          ),
                        )
                      }}
                    >
                      <Copy className="size-3.5" aria-hidden="true" />
                      {t("decks.duplicate")}
                    </Button>
                    <Button
                      size="sm"
                      aria-label={t("decks.renameNamed", { name: deck.name })}
                      onClick={() => {
                        setRenaming(deck)
                        setRenameValue(deck.name)
                      }}
                    >
                      <Pencil className="size-3.5" aria-hidden="true" />
                      {t("decks.rename")}
                    </Button>
                    <Button
                      size="sm"
                      variant="danger"
                      aria-label={t("decks.deleteNamed", { name: deck.name })}
                      onClick={() => {
                        setDeleting(deck)
                      }}
                    >
                      <Trash2 className="size-3.5" aria-hidden="true" />
                      {t("decks.delete")}
                    </Button>
                  </div>
                </div>
              </li>
            )
          })}
        </ul>
      )}
      <NewDeckDialog
        open={newOpen}
        onClose={() => {
          setNewOpen(false)
        }}
        defaultEdition={edition}
        onEditionChange={(next) => {
          // Only the dialog's own history entry names the edition, so leaving it drops `ed` again.
          const params = new URLSearchParams(location.search)
          params.set("ed", next)
          void navigate(
            { pathname: location.pathname, search: `?${params.toString()}` },
            { replace: true, state: location.state as unknown },
          )
        }}
        defaultName={t("decks.newNamePlaceholder", { n: decks.length + 1 })}
        classOptions={labels.classOptions}
        titles={labels.titles}
        titlesEdition={labels.titlesEdition}
        titleName={labels.titleName}
        onCreate={(input) => {
          const deck = createDeck(input, new Date())
          persist(decksStore.save(deck))
          // The new deck takes the dialog's history entry, so back returns to the list.
          void navigate(deckPath(deck), { replace: true })
        }}
      />
      <Dialog
        open={deleting !== null}
        onClose={closeDelete}
        title={t("decks.deleteTitle")}
        variant="center"
        footer={
          <div className="flex gap-2">
            <Button
              variant="danger"
              onClick={() => {
                if (deleting) persist(decksStore.remove(deleting.id))
                closeDelete()
              }}
            >
              {t("decks.delete")}
            </Button>
            <Button variant="ghost" onClick={closeDelete}>
              {t("settings.resetCancel")}
            </Button>
          </div>
        }
      >
        <p className="pb-2 text-14 text-text-2">
          {t("decks.deleteHint", { name: deleting?.name ?? "" })}
        </p>
      </Dialog>
      <Dialog
        open={renaming !== null}
        onClose={closeRename}
        title={t("decks.rename")}
        variant="center"
        footer={
          <div className="flex gap-2">
            <Button
              variant="primary"
              disabled={renameValue.trim() === ""}
              onClick={() => {
                const name = renameValue.trim()
                if (renaming && name !== "" && name !== renaming.name)
                  persist(decksStore.save(renameDeck(renaming, name, new Date())))
                closeRename()
              }}
            >
              {t("decks.renameConfirm")}
            </Button>
            <Button variant="ghost" onClick={closeRename}>
              {t("settings.resetCancel")}
            </Button>
          </div>
        }
      >
        <label className="flex flex-col gap-1 pb-2 text-13 text-text-2">
          {t("decks.nameLabel")}
          <input
            value={renameValue}
            onChange={(event) => {
              setRenameValue(event.target.value)
            }}
            className="h-11 rounded-button border border-border bg-surface-1 px-3 text-15 text-text-1"
          />
        </label>
      </Dialog>
    </div>
  )
}
