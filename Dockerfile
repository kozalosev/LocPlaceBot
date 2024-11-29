FROM rust:alpine as builder
WORKDIR /build

RUN apk update && apk add --no-cache musl-dev protobuf-dev

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

COPY src/ src/
COPY user-service-proto/ user-service-proto/
COPY locales/ locales/
COPY Cargo.* build.rs ./

ENV RUSTFLAGS='-C target-feature=-crt-static'
RUN cargo build --release && mv target/release/loc-place-bot /locPlaceBot

FROM alpine
RUN apk update && apk add --no-cache libgcc
COPY --from=builder /locPlaceBot /usr/local/bin/
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
ARG GAPI_MODE
ARG MSG_LOC_LIMIT
ARG WEBHOOK_URL
ARG REDIS_HOST
ARG REDIS_PORT
ARG REDIS_PASSWORD
ARG REQUESTS_LIMITER_MAX_ALLOWED
ARG REQUESTS_LIMITER_TIMEFRAME
ARG GRPC_ADDR_USER_SERVICE
ARG USER_CACHE_TIME_SECS
ARG CACHE_CLEAN_UP_INTERVAL_SECS
ARG SEARCH_RADIUS_METERS
ARG QUERY_CHECK_MODE
ENTRYPOINT [ "/usr/local/bin/locPlaceBot" ]

LABEL org.opencontainers.image.source=https://github.com/kozalosev/LocPlaceBot
LABEL org.opencontainers.image.description="Attach a location by its coordinates or name"
LABEL org.opencontainers.image.licenses=MIT
