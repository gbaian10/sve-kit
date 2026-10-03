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
        match: (key: string | Request, options?: CacheQueryOptions) => {
          const url = typeof key === "string" ? key : key.url
          const matched = options?.ignoreSearch
            ? [...cache.keys()].find((candidate) => candidate.split("?")[0] === url.split("?")[0])
            : url
          return Promise.resolve(cache.get(matched ?? "")?.clone())
        },
        keys: () => Promise.resolve([...cache.keys()].map((url) => new Request(url))),
        delete: (key: string | Request) =>
          Promise.resolve(cache.delete(typeof key === "string" ? key : key.url)),
        put: (key: string | Request, response: Response) => {
          if (failWrites) return Promise.reject(new Error("quota"))
          cache.set(typeof key === "string" ? key : key.url, response.clone())
          return Promise.resolve()
        },
      } as unknown as Cache)
    },
  } as unknown as CacheStorage
}
