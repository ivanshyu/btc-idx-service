FROM debian:bullseye-slim AS build

WORKDIR /opt/bis

ARG DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install git pkg-config curl build-essential openssl libssl-dev protobuf-compiler -y
RUN --mount=type=cache,target=/root/.cargo \
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs > rustup.sh \
    && chmod +x ./rustup.sh \
    && ./rustup.sh -y

COPY . .
RUN --mount=type=cache,target=/root/.cargo \
    --mount=type=cache,target=/opt/bis/target \
    --mount=type=secret,id=gitconfig,target=/root/.gitconfig \
    --mount=type=secret,id=git-credentials,target=/root/.git-credentials \
    . $HOME/.profile && cargo build --release \
    && strip /opt/bis/target/release/bis \
    && cp /opt/bis/target/release/bis /opt/bis

# Final Container
# FROM alpine AS build
FROM debian:bullseye-slim AS bis 

RUN apt-get update && apt-get install openssl ca-certificates -y

COPY --from=build /opt/bis/bis /usr/local/bin/bis

WORKDIR /app

ENTRYPOINT ["bis"]
