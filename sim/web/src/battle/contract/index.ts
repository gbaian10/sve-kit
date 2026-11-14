// The battle contract: wire types and validators generated from sim/battle-contract.
// Run `bun run generate:contract` to produce ./generated (every package script that needs it does).
export type * from "./generated/types"
export {
  validateBattleObservation,
  validateClientMessage,
  validateHostMessage,
  validateQuestion,
} from "./generated/validate.js"
