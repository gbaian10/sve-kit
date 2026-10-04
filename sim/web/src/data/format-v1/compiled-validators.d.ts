// Clean-checkout tooling needs these types before the build generates executable validators.
declare module "#snapshot-validators" {
  import type { ValidateFunction } from "ajv"
  const validators: Record<string, ValidateFunction>
  export default validators
}

declare module "#snapshot-conformance" {
  import type { ValidateFunction } from "ajv"
  const validators: Record<string, ValidateFunction>
  export default validators
}
