FROM rust:1.85-alpine3.21 AS builder
WORKDIR /app
RUN apk add --no-cache musl-dev
COPY . .
RUN cargo build --release -p emwin-cli

FROM alpine:3.21
LABEL org.opencontainers.image.description="EMWIN CLI with stream, server, download, inspect, and relay subcommands"

RUN addgroup -S emwin && adduser -S -G emwin emwin
COPY --from=builder /app/target/release/emwin-cli /usr/local/bin/emwin

USER emwin
ENTRYPOINT ["/usr/local/bin/emwin"]
