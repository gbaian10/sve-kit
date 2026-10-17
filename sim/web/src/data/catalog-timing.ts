let sequence = 0

/** Bounded User Timing entries; observers and DevTools still receive cleared measures. */
export function catalogWork<T>(stage: string, work: () => T): T {
  const name = `sve:catalog:${stage}`
  const start = `${name}:${String(++sequence)}`
  performance.mark(start)
  try {
    return work()
  } finally {
    performance.measure(name, start)
    performance.clearMarks(start)
    performance.clearMeasures(name)
  }
}
