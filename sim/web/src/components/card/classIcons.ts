import abyss from "../../assets/official/class/abyss.svg"
import dragon from "../../assets/official/class/dragon.svg"
import forest from "../../assets/official/class/forest.svg"
import haven from "../../assets/official/class/haven.svg"
import neutral from "../../assets/official/class/neutral.svg"
import rune from "../../assets/official/class/rune.svg"
import sword from "../../assets/official/class/sword.svg"
import type { DesignClass } from "../../domain/classes"

export const CLASS_ICON: Record<DesignClass, string> = {
  forest,
  sword,
  rune,
  dragon,
  abyss,
  haven,
  neutral,
}
