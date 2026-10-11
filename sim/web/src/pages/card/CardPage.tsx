import { ArrowLeft, Link2 } from "lucide-react"
import { useMemo, useState } from "react"
import { useTranslation } from "react-i18next"
import { Link as RouterLink, useLocation, useNavigate, useParams } from "react-router"

import { useCatalog, useImageIndex, useSearchReadiness } from "../../app/snapshot"
import { StatusStrip } from "../../app/StatusStrip"
import { CardImage } from "../../components/card/CardImage"
import { CardNameHeading, PlainNameHeading } from "../../components/card/CardNameHeading"
import { CardText } from "../../components/card/CardText"
import { CLASS_ICON } from "../../components/card/classIcons"
import {
  ATTACK_ICON,
  costIcon,
  costIconNeedsNumber,
  DEFENSE_ICON,
} from "../../components/card/symbolIcons"
import { type VocabularyKind, vocabularyLabel } from "../../components/card/vocabulary"
import { Badge } from "../../components/ui/Badge"
import { cn } from "../../components/ui/cn"
import { Link } from "../../components/ui/Link"
import { Segmented } from "../../components/ui/Segmented"
import { useToast } from "../../components/ui/toast-context"
import {
  type CardSummary,
  type Catalog,
  type Row,
  sharedAnnotatedTextResolver,
  stringValue,
} from "../../data"
import { classStyle } from "../../domain/classes"
import { displayName, type NameSource, UI_TEXT_LANG } from "../../domain/nameDisplay"
import type { Region, TextLang } from "../../domain/search"
import { currentUiLanguage } from "../../i18n"
import { usePrefs } from "../../settings"
import { PageTitle } from "../PageTitle"

function cardNoColumns(cardNo: string): number {
  // Symbols such as Ⓢ can use wide fallback glyphs despite the monospaced Latin digits.
  return Array.from(cardNo).reduce(
    (width, character) => width + (character.charCodeAt(0) <= 0x7f ? 1 : 2),
    0,
  )
}

// The header already shows the current name; the printed view adds the printed name and flavor.
const CURRENT_FIELDS = ["effect"] as const
const PRINTED_FIELDS = ["name", "effect", "flavor"] as const
const ICON_BUTTON =
  "flex size-11 shrink-0 items-center justify-center rounded-button text-text-1 hover:bg-surface-2"

// Catalog summaries only cover front faces; each back face needs its own selected name.
function backFaceName(
  index: Catalog["index"],
  revision: Row | undefined,
  summary: CardSummary,
): NameSource {
  const unit = revision ? index.textUnit(stringValue(revision["name_unit_id"])) : undefined
  if (!unit)
    return {
      original: { lang: summary.region === "jp" ? "ja" : "en", text: summary.cardNo },
      translations: {},
    }
  const translations: NameSource["translations"] = {}
  const translationQuality: NonNullable<NameSource["translationQuality"]> = {}
  for (const entry of revision?.["translations"] as Row[]) {
    if (entry["field"] !== "name") continue
    const target = stringValue(entry["target_lang"]) as TextLang
    const translation = index.translation(stringValue(entry["translation_id"]))
    const translatedUnit = translation
      ? index.textUnit(stringValue(translation["text_unit_id"]))
      : undefined
    if (!translatedUnit) continue
    translations[target] = stringValue(translatedUnit["text"])
    translationQuality[target] = {
      lowConfidence: translation?.["low_confidence"] === true,
      jpSource: entry["basis"] === "jp_source",
    }
  }
  return {
    original: { lang: stringValue(unit["lang"]) as TextLang, text: stringValue(unit["text"]) },
    translations,
    translationQuality,
  }
}

// Design 06: three 44px cells, radius 10, icon 20–24 beside a bold tabular number.
function StatCell({
  icon,
  label,
  value,
  showNumber = true,
}: {
  readonly icon: string
  readonly label: string
  readonly value: number | null
  readonly showNumber?: boolean
}) {
  const { t } = useTranslation()
  return (
    <span className="flex h-11 items-center justify-center gap-1 rounded-control border border-border bg-surface-1 text-16 font-bold tabular-nums">
      {value === null ? (
        <span className="text-12 font-medium text-text-3">
          {label} {t("cardPage.noValue")}
        </span>
      ) : (
        <>
          <img src={icon} alt={label} className="size-6" />
          {showNumber && <span>{value}</span>}
        </>
      )}
    </span>
  )
}

export function CardPage() {
  const { t } = useTranslation()
  const { cardNo, intId } = useParams()
  const { catalog, client } = useCatalog()
  const images = useImageIndex(client, catalog !== null)
  const prefs = usePrefs()
  const readiness = useSearchReadiness(prefs.cardEdition)
  const navigate = useNavigate()
  const location = useLocation()
  const toast = useToast()
  const [textMode, setTextMode] = useState<"current" | "printed">("current")
  const { cardEdition } = prefs
  const uiLanguage = currentUiLanguage(prefs.uiLanguage)
  const textLang = UI_TEXT_LANG[uiLanguage]
  const snapshot = client.snapshot()
  const pageKey = cardNo ?? intId ?? ""
  const resolver = useMemo(
    () => (snapshot && pageKey ? sharedAnnotatedTextResolver(client) : null),
    [client, snapshot, pageKey],
  )
  const printing = cardNo
    ? (catalog?.index.printingByCardNo(cardEdition, cardNo) ??
      catalog?.index.printingByCardNo(cardEdition === "jp" ? "en" : "jp", cardNo))
    : catalog?.index.cards
        .flatMap((card) => catalog.index.printingsOf(card["id"] as string))
        .find((row) => typeof row["int_id"] === "number" && String(row["int_id"]) === intId)
  const region = (printing?.["region"] as Region | undefined) ?? cardEdition
  const faces = printing ? (catalog?.index.facesOf(printing["card_id"] as string) ?? []) : []
  const summary: CardSummary | undefined = printing
    ? catalog?.summary(stringValue(printing["id"]))
    : undefined
  const card = printing ? catalog?.index.card(stringValue(printing["card_id"])) : undefined
  const family = card ? catalog?.index.family(stringValue(card["home_set_id"])) : undefined
  const setCode = family ? stringValue(family["public_code"]) : undefined
  // Opened from a list: back pops to it; opened directly: back is a link to the search page.
  const fromList =
    typeof location.state === "object" && location.state !== null && "background" in location.state
  const copyLink = async () => {
    try {
      await navigator.clipboard.writeText(window.location.href)
      toast.show({ message: t("cardPage.linkCopied") })
    } catch {
      toast.show({ message: t("cardPage.linkCopyFailed") })
    }
  }
  const index = catalog?.index
  const label = (kind: VocabularyKind, code: string) =>
    index ? vocabularyLabel(index, kind, code, textLang) : code
  const joiner = t("cardPage.separator")
  const metaLine = (classCode: string | null, typeCode: string | null) =>
    [
      classCode === null ? t("search.neutral") : label("class", classCode),
      ...(typeCode === null ? [] : [label("type", typeCode)]),
    ].join(joiner)
  const traitLabels = (codes: readonly string[]) =>
    codes.map((code) => label("trait", code)).join(joiner)

  if (catalog !== null && !printing) {
    return (
      <>
        <PageTitle>{t("cardPage.notFound")}</PageTitle>
        <p className="text-15 text-text-2">{t("cardPage.notFoundHint")}</p>
        <p className="mt-4">
          <Link to="/cards">{t("nav.cards")}</Link>
        </p>
      </>
    )
  }
  return (
    <article className="mx-auto w-full max-w-200">
      <StatusStrip readiness={readiness} />
      {/* Top row (design 06): back 44, set tag 28, number centred in Plex 600 15px, copy link 44. */}
      <div className="flex h-13 items-center gap-1">
        {fromList ? (
          <button
            type="button"
            aria-label={t("cardPage.back")}
            title={t("cardPage.back")}
            onClick={() => void navigate(-1)}
            className={ICON_BUTTON}
          >
            <ArrowLeft className="size-5.5" aria-hidden="true" />
          </button>
        ) : (
          <RouterLink
            to="/cards"
            aria-label={t("cardPage.back")}
            title={t("cardPage.back")}
            className={ICON_BUTTON}
          >
            <ArrowLeft className="size-5.5" aria-hidden="true" />
          </RouterLink>
        )}
        {setCode !== undefined && (
          <RouterLink
            to={`/sets/${encodeURIComponent(setCode)}`}
            aria-label={t("cardPage.set", { code: setCode })}
            className="flex h-7 shrink-0 items-center rounded-sm bg-surface-2 px-2 text-12 font-semibold text-text-2 hover:bg-surface-3"
          >
            {setCode}
          </RouterLink>
        )}
        <h1 className="min-w-0 flex-1 truncate text-center font-sans text-15 font-semibold tabular-nums">
          {cardNo ?? intId}
        </h1>
        <button
          type="button"
          aria-label={t("cardPage.copyLink")}
          title={t("cardPage.copyLink")}
          onClick={() => void copyLink()}
          className={ICON_BUTTON}
        >
          <Link2 className="size-5" aria-hidden="true" />
        </button>
      </div>
      {faces.map((face, ordinal) => {
        const faceId = face["id"] as string
        const wording = catalog?.index.wording(faceId, region)
        const revision = catalog?.index.displayRevision(faceId, region)
        const nameUnit = revision
          ? catalog?.index.textUnit(revision["name_unit_id"] as string)?.["text"]
          : undefined
        const undated = ((wording?.["undated_printing_ids"] ?? []) as string[]).map(
          (id) => (catalog?.index.printing(id)?.["card_no"] as string | undefined) ?? id,
        )
        const faceSummary: CardSummary | undefined =
          summary &&
          (ordinal === 0
            ? summary
            : {
                ...summary,
                faceId,
                name: catalog ? backFaceName(catalog.index, revision, summary) : summary.name,
                wordingPending: wording !== undefined,
              })
        const name = faceSummary
          ? displayName(faceSummary.name, uiLanguage, prefs.nameDisplay)
          : undefined
        const classCode =
          typeof revision?.["class_code"] === "string" ? revision["class_code"] : null
        const typeCode = typeof revision?.["type_code"] === "string" ? revision["type_code"] : null
        const traits = traitLabels((revision?.["traits"] ?? []) as string[])
        const cost = typeof revision?.["cost"] === "number" ? revision["cost"] : null
        const attack = typeof revision?.["attack"] === "number" ? revision["attack"] : null
        const defense = typeof revision?.["defense"] === "number" ? revision["defense"] : null
        const meta = metaLine(classCode, typeCode)
        return (
          <section
            key={faceId}
            aria-label={name?.primary.text ?? faceId}
            className={cn(ordinal > 0 && "mt-6 border-t border-border pt-4")}
          >
            {/* First screen (design 06): 96px thumbnail, name, class·type·traits, three stat cells. */}
            {faceSummary && name && (
              <div className="mt-1 flex gap-3.5">
                <CardImage
                  summary={faceSummary}
                  name={name.primary}
                  images={images}
                  alt="identify"
                  sizes="96px"
                  // Without self-start the row stretches the frame past 63:88 and the cover crop
                  // cuts the card's sides whenever the name column is taller than the image.
                  className="w-24 shrink-0 self-start"
                />
                <div className="flex min-w-0 flex-1 flex-col gap-1.5">
                  {ordinal > 0 && <Badge className="self-start">{t("cardPage.backFace")}</Badge>}
                  {revision ? (
                    <CardNameHeading
                      client={client}
                      edition={region}
                      resolver={resolver}
                      owner={{ kind: "face_revision", id: stringValue(revision["id"]) }}
                      fallback={name}
                    />
                  ) : (
                    <PlainNameHeading name={name} />
                  )}
                  {/* No revision (wording pending) means class and type are unknown, not neutral. */}
                  {revision && (
                    <p className="flex flex-wrap items-center gap-x-1.5 gap-y-0.5 text-13 text-text-2">
                      <img src={CLASS_ICON[classStyle(classCode)]} alt="" className="size-4.5" />
                      <span>{meta}</span>
                      {traits !== "" && <span className="text-text-3">{traits}</span>}
                    </p>
                  )}
                  <div className="mt-0.5 grid grid-cols-3 gap-1.5">
                    <StatCell
                      icon={costIcon(cost ?? 0)}
                      label={
                        cost !== null && !costIconNeedsNumber(cost)
                          ? t("cardPage.costValue", { value: cost })
                          : t("cardPage.stats.cost")
                      }
                      value={cost}
                      showNumber={cost !== null && costIconNeedsNumber(cost)}
                    />
                    <StatCell
                      icon={ATTACK_ICON}
                      label={t("cardPage.stats.attack")}
                      value={attack}
                    />
                    <StatCell
                      icon={DEFENSE_ICON}
                      label={t("cardPage.stats.defense")}
                      value={defense}
                    />
                  </div>
                </div>
              </div>
            )}
            {faceSummary?.wordingPending && (
              <Badge className="mt-2">{t("card.wordingPending")}</Badge>
            )}
            {printing &&
              (textMode === "printed" || revision) &&
              (textMode === "printed" ? PRINTED_FIELDS : CURRENT_FIELDS).map((field) => (
                <CardText
                  key={field}
                  edition={region}
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
            {ordinal === 0 && printing && (
              <Segmented
                className="mt-4"
                size="sm"
                label={t("card.textMode")}
                options={[
                  { value: "current", label: t("card.currentText") },
                  { value: "printed", label: t("card.printedText") },
                ]}
                value={textMode}
                onChange={setTextMode}
              />
            )}
            {wording && (
              <section className="mt-4 space-y-2" aria-label={t("card.wordingPending")}>
                <h3 className="text-14 font-semibold">{t("card.wordingPending")}</h3>
                {typeof nameUnit === "string" && <p>{nameUnit}</p>}
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
          </section>
        )
      })}
    </article>
  )
}
