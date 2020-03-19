# Build 
FROM rust:1.42-alpine as build

RUN apk add --no-cache musl-dev

COPY ./ ./

ENV RUSTFLAGS="-C target-feature=-crt-static"
RUN cargo build --release

RUN mkdir -p /build-out

RUN cp target/release/message_in_a_bottle /build-out/

# Run
FROM alpine:3.11.3

RUN apk add --no-cache gcc
COPY --from=build /build-out/message_in_a_bottle /

RUN adduser -D myuser
USER myuser

CMD /message_in_a_bottle ${PORT}