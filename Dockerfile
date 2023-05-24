FROM rust:alpine as builder
WORKDIR /build

RUN apk update && apk add --no-cache pkgconfig musl-dev libressl-dev

# Create an unprivileged user
ENV USER=appuser
ENV UID=10001
RUN adduser \
    --disabled-password \
    --gecos "" \
    --home "/nonexistent" \
    --shell "/sbin/nologin" \
    --no-create-home \
    --uid "${UID}" \
    "${USER}"

COPY src/* src/
COPY Cargo.* ./

ENV RUSTFLAGS='-C target-feature=-crt-static'
RUN cargo build --release && mv target/release/loc-place-bot /loc-place-bot

FROM alpine
RUN apk update && apk add --no-cache libgcc libressl
COPY --from=builder /loc-place-bot /usr/local/bin/
# Import the user and group files from the builder
COPY --from=builder /etc/passwd /etc/passwd
COPY --from=builder /etc/group /etc/group
# Use the unprivileged user
USER appuser:appuser

EXPOSE 8080
ARG TELOXIDE_TOKEN
ARG GOOGLE_MAPS_API_KEY
ARG RUST_LOG
ARG CACHE_TIME
ENTRYPOINT [ "/usr/local/bin/loc-place-bot" ]

LABEL org.opencontainers.image.source=https://github.com/kozalosev/LocPlaceBot
LABEL org.opencontainers.image.description="Attach a location by its coordinates or name"
LABEL org.opencontainers.image.licenses=MIT
