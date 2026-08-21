# Web GUI

Svelte + Vite frontend for Play and Debug. The Rust binary (`cargo run -- serve`) serves the API and, in production, the built `web/dist`.

## Build

From the repo root (after `npm install` in `web/`):

```bash
cd web && npm run build && cd ..
cargo run -- serve          # http://127.0.0.1:3000
```

## Development (hot reload)

```bash
# terminal 1
cargo run -- serve

# terminal 2
cd web && npm run dev       # http://127.0.0.1:5173 (proxies /api)
```

## Usage

Open the UI, switch **Play** / **Debug**, load games, click pieces to move.

In **Play**, the **Alpha-beta** panel sets depth / model / time for `ab`. **Runs** can start `ab vs ab` (or vs `mi`) autoplay and stop/save when done.

The game list includes `games/` and `data/raw/games/`. Loading a v2 training game applies its embedded start position (not always the opening).

Coordinates in the UI are shogi-style (file 1 = rightmost, rank 1 = top). Engine JSON / TSFEN stay 0-based — see [`src/README.md`](../src/README.md).
