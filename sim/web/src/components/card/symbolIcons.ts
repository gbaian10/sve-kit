import iconCost00 from "../../assets/card-labels/cost/cost-00.svg"
import iconCost01 from "../../assets/card-labels/cost/cost-01.svg"
import iconCost02 from "../../assets/card-labels/cost/cost-02.svg"
import iconCost03 from "../../assets/card-labels/cost/cost-03.svg"
import iconCost04 from "../../assets/card-labels/cost/cost-04.svg"
import iconCost05 from "../../assets/card-labels/cost/cost-05.svg"
import iconCost06 from "../../assets/card-labels/cost/cost-06.svg"
import iconCost07 from "../../assets/card-labels/cost/cost-07.svg"
import iconCost08 from "../../assets/card-labels/cost/cost-08.svg"
import iconCost09 from "../../assets/card-labels/cost/cost-09.svg"
import iconCost10 from "../../assets/card-labels/cost/cost-10.svg"
import iconCostX from "../../assets/card-labels/cost/cost-x.svg"
import iconHp from "../../assets/official/texticon/icon_hp.png"
import iconPower from "../../assets/official/texticon/icon_power.png"

// Our own cost crystals (blank crystal plus the number), one file per value 0–10; anything else
// (the real data has costs up to 15) shows the X crystal with the number written next to it by
// the caller.
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
