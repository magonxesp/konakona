# Konakona

Lightweight yt-dlp web wrapper for download videos quickily

## Build

Install dependencies

```sh
npm --prefix web install
```

Build frontend

```sh
npm --prefix web run build
```

Build binary execurable

```sh
cargo build --release
```

## Run

### Using the binary executable

```sh
./target/release/konakona
```

### Using docker compose

```sh
docker compose up -d
```
