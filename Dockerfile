FROM rust:1.65.0-bullseye as build-env
RUN apt update && apt install -y libssl-dev pkg-config build-essential cmake

WORKDIR /app
COPY . /app
RUN cargo build --bin app-cli --release --features=cli,umbrel,git

FROM gcr.io/distroless/cc
COPY --from=build-env /app/target/release/app-cli /

CMD ["/app-cli"]
