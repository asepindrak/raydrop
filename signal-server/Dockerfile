FROM rust:1.82-alpine AS builder
RUN apk add --no-cache musl-dev
WORKDIR /app
COPY Cargo.toml ./
COPY src ./src
RUN cargo build --release

FROM alpine:3.20
RUN apk add --no-cache ca-certificates
WORKDIR /app
COPY --from=builder /app/target/release/raydrop-signal-server /usr/local/bin/raydrop-signal-server
EXPOSE 8080/tcp
EXPOSE 3478/udp
ENV SIGNAL_HTTP_ADDR=0.0.0.0:8080
ENV SIGNAL_UDP_ADDR=0.0.0.0:3478
CMD ["raydrop-signal-server"]

