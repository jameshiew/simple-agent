FROM clux/muslrust:1.83.0-stable-2024-12-27 AS build

ARG TARGETPLATFORM

RUN cargo install cargo-chef --locked

ENV CARGO_TARGET_DIR=/target

COPY recipe.json .
RUN cargo chef cook --recipe-path recipe.json

COPY Cargo.lock Cargo.toml crates/* ./

RUN if [ "${TARGETPLATFORM}" = "linux/arm64" ]; then \
    rustup target add aarch64-unknown-linux-musl && \
    cargo build --bin simple-agent --target aarch64-unknown-linux-musl; \
    elif [ "${TARGETPLATFORM}" = "linux/amd64" ]; then \
    rustup target add x86_64-unknown-linux-musl && \
    cargo build --bin simple-agent --target x86_64-unknown-linux-musl; \
    else \
    echo "Unsupported TARGETPLATFORM: ${TARGETPLATFORM}" && exit 1; \
    fi

RUN if [ "${TARGETPLATFORM}" = "linux/arm64" ]; then \
    cp /target/aarch64-unknown-linux-musl/debug/simple-agent /simple-agent; \
    elif [ "${TARGETPLATFORM}" = "linux/amd64" ]; then \
    cp /target/x86_64-unknown-linux-musl/debug/simple-agent /simple-agent; \
    else \
    echo "Unsupported TARGETPLATFORM: ${TARGETPLATFORM}" && exit 1; \
    fi

RUN ls /target

FROM debian:12

RUN apt-get update && apt-get install -y \
    binutils \
    curl \
    file \
    git \
    golang \
    python3-dev \
    strace \
    sudo

RUN adduser --disabled-password --gecos '' agent \
    && adduser agent sudo \
    && echo '%sudo ALL=(ALL) NOPASSWD:ALL' >> /etc/sudoers \
    && mkdir /system && chown agent:agent /system \
    && mkdir /workspace && chown agent:agent /workspace

COPY --from=build /simple-agent /usr/local/bin/simple-agent

USER agent
WORKDIR /workspace

CMD ["simple-agent"]