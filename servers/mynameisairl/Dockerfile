# Stage 1: Build
FROM ubuntu:24.04 AS builder
RUN apt-get update && apt-get install -y curl build-essential llvm-18-dev libz-dev pkg-config cmake python3
# Install Rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"
# Copy AIRL repo and build g3
COPY airl/ /build/airl/
WORKDIR /build/airl
RUN cargo build -p airl-rt && cargo clean -p airl-runtime && cargo build --release --features aot && bash scripts/build-g3.sh
# Copy AirTraffic sources
COPY airtraffic/ /build/airtraffic/
# Copy mynameisairl sources
COPY mynameisairl/ /build/mynameisairl/
# Generate embedded guide + version, then compile
RUN AIRL_DIR=/build/airl G3=/build/airl/g3 AT_ROOT=/build/airtraffic \
    AIRL_STDLIB=/build/airl/stdlib \
    bash /build/mynameisairl/build.sh /build/mynameisairl/mynameisairl

# Stage 2: Runtime
FROM ubuntu:24.04
COPY --from=builder /build/mynameisairl/mynameisairl /usr/local/bin/mynameisairl
# Guide is now embedded in the binary at build time — no file mount needed
ENTRYPOINT ["mynameisairl"]
