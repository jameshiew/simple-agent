FROM clux/muslrust:1.83.0-stable-2024-12-27 AS build

RUN cargo install cargo-chef --locked

ENV CARGO_TARGET_DIR=/target
COPY recipe.json .
RUN cargo chef cook --recipe-path recipe.json

COPY Cargo.lock Cargo.toml crates/* ./

RUN cargo build --bin simple-agent
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

RUN adduser --disabled-password --gecos '' agent
RUN adduser agent sudo
RUN echo '%sudo ALL=(ALL) NOPASSWD:ALL' >> /etc/sudoers

RUN mkdir /system && chown agent:agent /system
RUN mkdir /workspace && chown agent:agent /workspace

USER agent
WORKDIR /workspace

COPY --from=build /target/aarch64-unknown-linux-musl/debug/simple-agent /usr/local/bin/simple-agent

CMD ["simple-agent"]