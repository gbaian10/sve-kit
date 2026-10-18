export function drain<T>(cursor: Iterator<void, T>): T {
  let step = cursor.next()
  while (!step.done) step = cursor.next()
  return step.value
}
