FROM rust:1.85-alpine3.21 AS builder
WORKDIR /app
RUN apk add --no-cache musl-dev
COPY . .
RUN cargo build --release -p byteblaster-cli

FROM alpine:3.21
LABEL org.opencontainers.image.description="ByteBlaster CLI with stream, server, download, inspect, and relay subcommands"

RUN addgroup -S byteblaster && adduser -S -G byteblaster byteblaster
COPY --from=builder /app/target/release/byteblaster-cli /usr/local/bin/byteblaster

USER byteblaster
ENTRYPOINT ["/usr/local/bin/byteblaster"]
