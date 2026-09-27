// case: src/settings may use storage -> none
export function read(): string | null {
  try {
    return localStorage.getItem("k")
  } catch {
    return null
  }
}
