# Builds the single binary with the web app embedded, and runs it as a non-root user with SQLite on /data.

FROM node:24-alpine AS web
WORKDIR /src/web
COPY web/package.json web/package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY web/ ./
COPY openapi.json /src/openapi.json
RUN npm run build

FROM rust:1.95-alpine AS server
RUN apk add --no-cache musl-dev
WORKDIR /src
COPY Cargo.toml Cargo.lock rustfmt.toml clippy.toml ./
COPY crates crates
COPY --from=web /src/web/dist web/dist
RUN cargo build --release --locked --bin upheap-server

FROM alpine:3.22
RUN addgroup -S heap && adduser -S -G heap heap && mkdir /data && chown heap:heap /data
COPY --from=server /src/target/release/upheap-server /usr/local/bin/upheap-server
USER heap
ENV DATABASE_URL="sqlite:///data/heap.db?mode=rwc" \
    BIND_ADDR="0.0.0.0:8080"
VOLUME /data
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=3s CMD wget -qO /dev/null http://127.0.0.1:8080/readyz || exit 1
ENTRYPOINT ["upheap-server"]
