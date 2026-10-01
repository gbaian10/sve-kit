import iconAct from "../../assets/official/texticon/icon_act.png"
import iconAdvanceStand from "../../assets/official/texticon/icon_advancestand.png"
import iconBishop from "../../assets/official/texticon/icon_bishop.png"
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
import iconDragon from "../../assets/official/texticon/icon_dragon.png"
import iconElf from "../../assets/official/texticon/icon_elf.png"
import iconEvolve from "../../assets/official/texticon/icon_evolve.png"
import iconFanfare from "../../assets/official/texticon/icon_fanfare.png"
import iconHp from "../../assets/official/texticon/icon_hp.png"
import iconLastword from "../../assets/official/texticon/icon_lastword.png"
import iconNeutral from "../../assets/official/texticon/icon_neutral.png"
import iconNightmare from "../../assets/official/texticon/icon_nightmare.png"
import iconPower from "../../assets/official/texticon/icon_power.png"
import iconQ from "../../assets/official/texticon/icon_q.png"
import iconQuick from "../../assets/official/texticon/icon_quick.png"
import iconRoyal from "../../assets/official/texticon/icon_royal.png"
import iconStand from "../../assets/official/texticon/icon_stand.png"
import iconSyokuzi from "../../assets/official/texticon/icon_syokuzi.png"
import iconUb from "../../assets/official/texticon/icon_UB.png"
import iconWitch from "../../assets/official/texticon/icon_witch.png"
import iconRide from "../../assets/official/texticon/ride.png"

// Official text icons by `text_symbol.code` (the snapshot names the symbol, this file only knows
// which file draws it). Cost is one icon per value; anything the snapshot adds later falls back to
// its copy text until an icon is mapped here.
const COST: Record<string, string> = {
  "0": iconCost00,
  "1": iconCost01,
  "2": iconCost02,
  "3": iconCost03,
  "4": iconCost04,
  "5": iconCost05,
  "6": iconCost06,
  "7": iconCost07,
  "8": iconCost08,
  "9": iconCost09,
  "10": iconCost10,
  X: iconCostX,
}

const PLAIN: Record<string, string> = {
  fanfare: iconFanfare,
  lastword: iconLastword,
  evolve: iconEvolve,
  power: iconPower,
  hp: iconHp,
  stand: iconStand,
  act: iconAct,
  advancestand: iconAdvanceStand,
  quick: iconQuick,
  ub: iconUb,
  syokuzi: iconSyokuzi,
  q: iconQ,
  ride: iconRide,
  elf: iconElf,
  royal: iconRoyal,
  witch: iconWitch,
  dragon: iconDragon,
  nightmare: iconNightmare,
  bishop: iconBishop,
  neutral: iconNeutral,
}

/** The icon for a symbol code and parameter, or undefined when none is mapped. */
export function symbolIcon(code: string, parameter?: string): string | undefined {
  if (code === "cost") return parameter === undefined ? undefined : COST[parameter]
  return PLAIN[code]
}
