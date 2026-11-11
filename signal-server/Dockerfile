FROM rust:alpine AS builder
RUN apk add --no-cache build-base musl-dev
WORKDIR /app
COPY Cargo.toml ./
COPY src ./src
RUN cargo build --release

FROM alpine:3.21
RUN apk add --no-cache ca-certificates
WORKDIR /app
RUN mkdir -p /data
VOLUME ["/data"]
COPY --from=builder /app/target/release/raydrop-signal-server /usr/local/bin/raydrop-signal-server
EXPOSE 8080/tcp
EXPOSE 3478/udp
ENV SIGNAL_HTTP_ADDR=0.0.0.0:8080
ENV SIGNAL_UDP_ADDR=0.0.0.0:3478
ENV SIGNAL_DB_PATH=/data/signal.db
CMD ["raydrop-signal-server"]
