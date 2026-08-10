# Supply only reviewed, digest-pinned images. Mutable defaults are intentionally absent.
ARG WEB_BUILD_IMAGE
ARG RUST_BUILD_IMAGE
ARG RUNTIME_IMAGE

FROM ${WEB_BUILD_IMAGE} AS web-build
WORKDIR /src
COPY VERSION ./VERSION
COPY web/package.json web/pnpm-lock.yaml ./web/
RUN pnpm --dir web install --frozen-lockfile
COPY web ./web
RUN pnpm --dir web run check && pnpm --dir web run test && pnpm --dir web run build

FROM ${RUST_BUILD_IMAGE} AS rust-build
WORKDIR /src
COPY VERSION ./VERSION
COPY rust/Cargo.toml rust/Cargo.lock ./rust/
COPY rust/src ./rust/src
COPY rust/static ./rust/static
RUN cargo build --locked --release --manifest-path rust/Cargo.toml

FROM ${RUNTIME_IMAGE} AS runtime
RUN apt-get update \
    && apt-get install --yes --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 app \
    && useradd --uid 10001 --gid 10001 --no-create-home --shell /usr/sbin/nologin app
WORKDIR /app
COPY --from=rust-build /src/rust/target/release/azure-sql-tco /usr/local/bin/azure-sql-tco
COPY --from=web-build /src/web/build ./web
ENV APP_ENV=production \
    HTTP_BIND=0.0.0.0:8080 \
    WEB_ASSET_DIR=/app/web
EXPOSE 8080
USER 10001:10001
ENTRYPOINT ["/usr/local/bin/azure-sql-tco"]