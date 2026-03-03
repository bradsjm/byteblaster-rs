FROM rust:1-alpine AS builder
WORKDIR /app
RUN apk add --no-cache musl-dev
COPY . .
RUN cargo build --release

FROM alpine:latest
LABEL org.opencontainers.image.description="ByteBlaster protocol decoding, client runtime, and CLI tooling"
COPY --from=builder /app/target/release/byteblaster-cli /byteblaster-cli
ENTRYPOINT ["/byteblaster-cli"]
