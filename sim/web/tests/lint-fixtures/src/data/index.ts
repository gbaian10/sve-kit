// case: src/data may fetch -> none
export async function loadSnapshot(): Promise<unknown> {
  const response = await fetch("/snapshot.json")
  return response.json()
}
