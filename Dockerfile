FROM rust:1.64.0-slim-bullseye as build-env

RUN apt update && apt install -y cmake build-essential

WORKDIR /app
COPY . /app
RUN cargo build --bin app-cli --release --features=cli,umbrel

FROM gcr.io/distroless/cc
COPY --from=build-env /app/target/release/app-cli /

CMD ["/app-cli"]
