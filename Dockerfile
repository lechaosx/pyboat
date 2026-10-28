FROM rust:1.98.1-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY tools/playground-capture/Cargo.toml tools/playground-capture/Cargo.toml
COPY tools/playground-capture/src tools/playground-capture/src
RUN cargo build --locked --release -p server

FROM debian:bookworm-slim
COPY --from=build /src/target/release/server /usr/local/bin/pyboat-server
EXPOSE 8765/udp
ENTRYPOINT ["pyboat-server"]
CMD ["0.0.0.0:8765", "/certificates/fullchain.pem", "/certificates/privkey.pem"]
