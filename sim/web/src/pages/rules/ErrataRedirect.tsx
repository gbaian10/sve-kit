import { Navigate, useLocation } from "react-router"

/** The errata list moved under /rules; shared links keep their query and hash. */
export function ErrataRedirect() {
  const { search, hash } = useLocation()
  return <Navigate replace to={{ pathname: "/rules/errata", search, hash }} />
}
