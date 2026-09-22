# sim

Battle simulator. Not started yet.

- `engine/` — rules engine shared by the server, the browser (WASM) and desktop builds
- `server/` — authoritative game server, served from the `ws.` subdomain (Rust preferred, undecided)
- `web/` — one TypeScript client for the card browser, deck builder and game board, on the main domain
