# sim

Battle simulator. Not started yet.

- `battle-contract/` — wire types of the battle contract; the web types and validator are generated from them
- `engine/` — rules engine shared by the server, the browser (WASM) and desktop builds
- `server/` — authoritative game server, served from the `ws.` subdomain (Rust preferred, undecided)
- `web/` — one TypeScript client for the card browser, deck builder and game board, on the main domain
