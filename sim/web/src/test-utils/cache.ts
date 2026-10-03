/** In-memory browser cache; namespace operations are observable without any network. */
export function memoryCache(failWrites = false): CacheStorage {
  const namespaces = new Map<string, Map<string, Response>>()
  return {
    keys: () => Promise.resolve([...namespaces.keys()]),
    delete: (name: string) => Promise.resolve(namespaces.delete(name)),
    open: (name: string) => {
      let values = namespaces.get(name)
      if (!values) {
        values = new Map()
        namespaces.set(name, values)
      }
      const cache = values
      return Promise.resolve({
        match: (key: string) => Promise.resolve(cache.get(key)?.clone()),
        put: (key: string, response: Response) => {
          if (failWrites) return Promise.reject(new Error("quota"))
          cache.set(key, response.clone())
          return Promise.resolve()
        },
      } as unknown as Cache)
    },
  } as unknown as CacheStorage
}
