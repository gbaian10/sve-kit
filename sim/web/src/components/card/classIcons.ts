import abyss from "../../assets/official/class/abyss.png"
import dragon from "../../assets/official/class/dragon.png"
import forest from "../../assets/official/class/forest.png"
import haven from "../../assets/official/class/haven.png"
import neutral from "../../assets/official/class/neutral.png"
import rune from "../../assets/official/class/rune.png"
import sword from "../../assets/official/class/sword.png"
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
