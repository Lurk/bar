FROM rust:latest AS build

WORKDIR /app

COPY Cargo.* ./
COPY src ./src/

RUN cargo build --release

FROM debian:bookworm as app

RUN apt-get update && apt-get install -y openssl ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=build /app/target/release/bar /usr/bin

ENTRYPOINT ["/usr/bin/bar"]
