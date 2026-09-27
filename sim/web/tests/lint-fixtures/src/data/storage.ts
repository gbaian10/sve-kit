// case: localStorage in src/data -> no-restricted-globals
export const cached = () => localStorage.getItem("snapshot")

// case: window.sessionStorage in src/data -> no-restricted-properties
export const session = () => window.sessionStorage.getItem("snapshot")
