import iconCost00 from "../../assets/official/texticon/icon_cost00.png"
import iconCost01 from "../../assets/official/texticon/icon_cost01.png"
import iconCost02 from "../../assets/official/texticon/icon_cost02.png"
import iconCost03 from "../../assets/official/texticon/icon_cost03.png"
import iconCost04 from "../../assets/official/texticon/icon_cost04.png"
import iconCost05 from "../../assets/official/texticon/icon_cost05.png"
import iconCost06 from "../../assets/official/texticon/icon_cost06.png"
import iconCost07 from "../../assets/official/texticon/icon_cost07.png"
import iconCost08 from "../../assets/official/texticon/icon_cost08.png"
import iconCost09 from "../../assets/official/texticon/icon_cost09.png"
import iconCost10 from "../../assets/official/texticon/icon_cost10.png"
import iconCostX from "../../assets/official/texticon/icon_costX.png"
import iconHp from "../../assets/official/texticon/icon_hp.png"
import iconPower from "../../assets/official/texticon/icon_power.png"

// Official cost crystals, one file per value 0–10; anything else (the real data has costs up to
// 15) shows the X crystal with the number written next to it by the caller.
const COST = [
  iconCost00,
  iconCost01,
  iconCost02,
  iconCost03,
  iconCost04,
  iconCost05,
  iconCost06,
  iconCost07,
  iconCost08,
  iconCost09,
  iconCost10,
]

export function costIcon(cost: number): string {
  return COST[cost] ?? iconCostX
}

/** True when the crystal itself does not show the number, so a caller must print it. */
export function costIconNeedsNumber(cost: number): boolean {
  return COST[cost] === undefined
}

export const ATTACK_ICON = iconPower
export const DEFENSE_ICON = iconHp
