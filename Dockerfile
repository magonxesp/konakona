FROM node:22 AS web-builder

WORKDIR /build

COPY web web

RUN npm --prefix web install
RUN npm --prefix web run build

FROM rust:1.93 AS builder

WORKDIR /build

COPY . .

RUN cargo build --release

FROM debian:13-slim

WORKDIR /app

RUN apt-get update \
  && apt-get install -y --no-install-recommends ffmpeg ca-certificates libopus0 python3 python3-pip \
  && python3 -m pip install --no-cache-dir --break-system-packages yt-dlp \
  && rm -rf /var/lib/apt/lists/*

RUN mkdir web

COPY --from=web-builder /build/web/dist ./web/dist
COPY --from=builder /build/target/release/konakona ./konakona

RUN chmod +x /app/konakona

EXPOSE 8080

CMD [ "/app/konakona" ]
