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
# Compile mynameisairl
RUN AIRL_STDLIB=/build/airl/stdlib ./g3 -- \
    /build/airtraffic/src/transport.airl \
    /build/airtraffic/src/jsonrpc.airl \
    /build/airtraffic/src/schema.airl \
    /build/mynameisairl/patch-prompts-list.airl \
    /build/airtraffic/src/airtraffic.airl \
    /build/mynameisairl/mynameisairl.airl \
    -o /build/mynameisairl/mynameisairl

# Stage 2: Runtime
FROM ubuntu:24.04
COPY --from=builder /build/mynameisairl/mynameisairl /usr/local/bin/mynameisairl
COPY --from=builder /build/airl/AIRL-LLM-Guide.md /data/AIRL-LLM-Guide.md
ENTRYPOINT ["mynameisairl"]
