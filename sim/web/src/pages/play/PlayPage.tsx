import { ArrowLeft } from "lucide-react"
import { useMemo } from "react"
import { useTranslation } from "react-i18next"
import { Link, useSearchParams } from "react-router"

import { useUiLanguageSync } from "../../app/language"
import { useThemeAttributes } from "../../app/theme-attributes"
import { type Scene, SCENES } from "../../battle/fixtures/scenes"
import { scriptedSource } from "../../battle/fixtures/scripted"
import { BattleScreen } from "./BattleScreen"

/**
 * The battle page. Until the local engine is wired in it plays fixture scenes: `?scene=` picks
 * one, and `?dual=1` puts both seats side by side, each on its own source, to compare what the
 * two players are told.
 */
export function PlayPage() {
  const { t } = useTranslation()
  useThemeAttributes()
  useUiLanguageSync()
  const [params, setParams] = useSearchParams()
  const scene: Scene = SCENES.find((entry) => entry.id === params.get("scene")) ?? SCENES[0]
  const dual = params.get("dual") === "1"
  // A scripted source holds nothing to release, so the page makes one per seat and lets it go;
  // closing one in an effect cleanup would hand a closed source back when React re-runs effects.
  const sources = useMemo(
    () => ({
      own: scriptedSource(scene, "p1"),
      opposite: dual ? scriptedSource(scene, "p2") : undefined,
    }),
    [scene, dual],
  )
  const update = (change: (next: URLSearchParams) => void) => {
    const next = new URLSearchParams(params)
    change(next)
    setParams(next, { replace: true })
  }
  return (
    <main>
      <h1 className="sr-only">{t("play.title")}</h1>
      <BattleScreen
        key={scene.id}
        source={sources.own}
        opposite={sources.opposite}
        leaders={{ p1: scene.leaders[0], p2: scene.leaders[1] }}
        slots={scene.slots}
        attackers={scene.attackers}
        onDual={(next) => {
          update((query) => {
            if (next) query.set("dual", "1")
            else query.delete("dual")
          })
        }}
        corner={
          <>
            <Link to="/" className="bp-chip-link">
              <ArrowLeft aria-hidden="true" className="bp-chip-icon" />
              {t("play.leave")}
            </Link>
            <label>
              <span className="bp-chip-label">{t("play.dev.scene")}</span>
              <select
                value={scene.id}
                onChange={(event) => {
                  update((query) => {
                    query.set("scene", event.target.value)
                  })
                }}
              >
                {SCENES.map((entry) => (
                  <option key={entry.id} value={entry.id}>
                    {t(`play.dev.scenes.${entry.id}`)}
                  </option>
                ))}
              </select>
            </label>
          </>
        }
      />
    </main>
  )
}
