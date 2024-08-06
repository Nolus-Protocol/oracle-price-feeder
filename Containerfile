ARG package

FROM docker.io/library/rust:latest AS compile-base

RUN ["apt-get", "update"]

RUN ["apt-get", "upgrade", "--purge", "--yes"]

RUN ["apt-get", "install", "--yes", "libc6-dev"]

USER "0":"0"

WORKDIR "/code/"

RUN ["chmod", "=555", "/code/"]

RUN ["mkdir", "-m", "=0555", "/code/src/"]

RUN ["touch", "/code/src/lib.rs"]

RUN ["mkdir", "-m", "=0777", "/code/target/"]

COPY --chown="0":"0" --chmod="0555" "./.cargo/" "/code/.cargo/"

COPY --chown="0":"0" --chmod="0555" "./Cargo.toml" "/code/Cargo.toml"

COPY --chown="0":"0" --chmod="0555" "./Cargo.lock" "/code/Cargo.lock"

ENV CARGO_INCREMENTAL="0"

USER "1000":"1000"

RUN ["cargo", "fetch", "--locked"]

FROM compile-base AS compile-lib-base

COPY --chown="0":"0" --chmod="0555" "./src/" "/code/src/"

FROM compile-lib-base AS compile-application-base

COPY --from=compile-lib-base --chown="0":"0" --chmod="0555" \
    "/code/src/lib.rs" \
    "/code/application/src/lib.rs"

ARG package

LABEL "package"="${package}"

COPY --chown="0":"0" --chmod="0555" \
    "./${package}/Cargo.toml" \
    "/code/application/Cargo.toml"

RUN ["cargo", "fetch", "--locked"]

USER "0":"0"

RUN ["rm", "-f", "/code/application/src/lib.rs"]

USER "1000":"1000"

FROM compile-application-base AS compile-application

ARG package

LABEL "package"="${package}"

ARG profile

LABEL "profile"="${profile}"

COPY --chown="0":"0" --chmod="0555" "./${package}/" "/code/application/"

RUN "cargo" \
        "rustc" \
        "--locked" \
        "--manifest-path" "/code/application/Cargo.toml" \
        "--profile" "${profile}" \
        "--target" "x86_64-unknown-linux-gnu" \
        "--" \
        "-C" "target-feature=+crt-static"

FROM gcr.io/distroless/static:latest AS service-base

VOLUME ["/service/logs/"]

WORKDIR "/service/"

ENTRYPOINT ["/service/service"]

ENV ADMIN_CONTRACT_ADDRESS="###"
ENV BROADCAST_DELAY_DURATION_SECONDS="2"
ENV BROADCAST_RETRY_DELAY_DURATION_MILLISECONDS="500"
ENV FEE_TOKEN_DENOM="unls"
ENV GAS_FEE_CONF__GAS_ADJUSTMENT_NUMERATOR="12"
ENV GAS_FEE_CONF__GAS_ADJUSTMENT_DENOMINATOR="10"
ENV GAS_FEE_CONF__GAS_PRICE_NUMERATOR="1"
ENV GAS_FEE_CONF__GAS_PRICE_DENOMINATOR="400"
ENV GAS_FEE_CONF__FEE_ADJUSTMENT_NUMERATOR="5"
ENV GAS_FEE_CONF__FEE_ADJUSTMENT_DENOMINATOR="1"
ENV IDLE_DURATION_SECONDS="60"
ENV LOGS_DIRECTORY="/service/logs/"
ENV NODE_GRPC_URI="###"
ENV OUTPUT_JSON="0"
ENV SIGNING_KEY_MNEMONIC="###"
ENV TIMEOUT_DURATION_SECONDS="60"

FROM service-base AS alarms-dispatcher-base

ENV PRICE_ALARMS_GAS_LIMIT_PER_ALARM="500000"
ENV PRICE_ALARMS_MAX_ALARMS_GROUP="32"
ENV TIME_ALARMS_GAS_LIMIT_PER_ALARM="500000"
ENV TIME_ALARMS_MAX_ALARMS_GROUP="32"

FROM alarms-dispatcher-base AS alarms-dispatcher

ARG profile_output_dir

COPY --from=compile-application --chown="0":"0" --chmod="0100" \
    "/code/target/x86_64-unknown-linux-gnu/${profile_output_dir}/alarms-dispatcher" \
    "./service"

FROM service-base AS market-data-feeder-base

ENV DURATION_BEFORE_START="600"
ENV GAS_LIMIT="###"
ENV UPDATE_CURRENCIES_INTERVAL_SECONDS="15"

ARG package
FROM ${package}-base AS service

ARG package

LABEL "package"="${package}"

ARG profile

LABEL "profile"="${profile}"

ARG profile_output_dir

COPY --from=compile-application --chown="0":"0" --chmod="0100" \
    "/code/target/x86_64-unknown-linux-gnu/${profile_output_dir}/${package}" \
    "./service"
