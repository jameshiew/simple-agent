FROM rust:1.85-bookworm AS build

ENV CARGO_TARGET_DIR=/target

RUN cargo install cargo-chef --locked --version 0.1.68

COPY recipe.json .
RUN cargo chef cook --recipe-path recipe.json

COPY Cargo.lock Cargo.toml crates/* ./
RUN cargo build --bin simple-agent && \
    cp /target/debug/simple-agent /usr/local/bin/simple-agent

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

COPY --from=build /usr/local/bin/simple-agent /usr/local/bin/simple-agent

USER agent
WORKDIR /workspace

CMD ["simple-agent"]