# Supply only reviewed, digest-pinned images. Mutable defaults are intentionally absent.
ARG WEB_BUILD_IMAGE
ARG RUST_DEPENDENCY_IMAGE
ARG RUNTIME_IMAGE

FROM ${RUST_DEPENDENCY_IMAGE} AS rust-build
WORKDIR /src
COPY VERSION ./VERSION
COPY rust/src ./rust/src
COPY rust/static ./rust/static
COPY app/catalogs ./app/catalogs
RUN cargo build --locked --release --manifest-path rust/Cargo.toml

FROM ${WEB_BUILD_IMAGE} AS web-build
WORKDIR /src
COPY VERSION ./VERSION
COPY web/package.json web/package-lock.json ./web/
COPY web/scripts ./web/scripts
RUN test "$(node --version)" = "v24.19.0" \
    && test "$(npm --version)" = "11.17.0" \
    && npm run lockfile:check --prefix web \
    && npm audit --prefix web --audit-level=high --registry=https://packagefeedproxy.microsoft.io/npm/ \
    && npm ci --prefix web --registry=https://packagefeedproxy.microsoft.io/npm/
COPY openapi ./openapi
COPY web ./web
RUN npm run api:generate --prefix web \
    && npm run lint --prefix web \
    && npm run check --prefix web \
    && npm run test --prefix web \
    && npm run build --prefix web

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