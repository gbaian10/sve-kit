import { createBrowserRouter, RouterProvider } from "react-router"

import { routes } from "./app/routes"

const router = createBrowserRouter(routes)

export function App() {
  return <RouterProvider router={router} />
}
