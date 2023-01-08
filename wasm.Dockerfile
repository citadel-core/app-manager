FROM --platform=linux/amd64 alpine:edge as build-env

RUN apk add ca-certificates gcc openssl-dev pkgconfig cmake clang musl-dev libc-dev git make llvm15 perl

ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH

RUN set -eux; \
    wget "https://static.rust-lang.org/rustup/dist/x86_64-unknown-linux-musl/rustup-init"; \
    chmod +x rustup-init; \
    ./rustup-init -y --no-modify-path --default-toolchain nightly; \
    rm rustup-init; \
    chmod -R a+w $RUSTUP_HOME $CARGO_HOME;

WORKDIR /wasi

RUN git clone https://github.com/WebAssembly/wasi-libc.git .

RUN make -j$(nproc) THREAD_MODEL=posix

RUN make install -j$(nproc) THREAD_MODEL=posix

RUN rustup target add wasm32-wasi

WORKDIR /app
COPY . /app
RUN cargo fetch -v
ENV CFLAGS="--sysroot=/wasi/sysroot"
RUN cargo build -v --bin app-cli --release --features=cli,umbrel --target wasm32-wasi

FROM scratch
COPY --from=build-env /app/target/wasm32-wasi/release/app-cli.wasm /

ENTRYPOINT ["app-cli.wasm"]
