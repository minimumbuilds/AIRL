# AIRL — Docker build with full MLIR/GPU support
#
# This image installs LLVM 19, MLIR, and libzstd so that the airl-mlir crate
# (which requires melior) compiles without any extra host setup.
#
# Build:
#   docker build -t airl .
#
# Run (interactive):
#   docker run --rm -it airl
#
# Execute an AIRL file from the host:
#   docker run --rm -v "$PWD":/work airl airl-driver run /work/myprogram.airl
#
# GPU support (requires NVIDIA container runtime):
#   docker run --rm --gpus all airl airl-driver run /work/myprogram.airl

FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive

# System dependencies
RUN apt-get update && apt-get install -y \
    curl \
    build-essential \
    cmake \
    python3 \
    pkg-config \
    llvm-19-dev \
    libmlir-19-dev \
    mlir-19-tools \
    libzstd-dev \
    libclang-19-dev \
    && rm -rf /var/lib/apt/lists/*

# Point melior at the installed LLVM 19
ENV MLIR_SYS_190_PREFIX=/usr/lib/llvm-19
ENV LLVM_SYS_190_PREFIX=/usr/lib/llvm-19
ENV PATH="/usr/lib/llvm-19/bin:${PATH}"

# Install Rust via rustup (stable toolchain)
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
ENV PATH="/root/.cargo/bin:${PATH}"

WORKDIR /airl
COPY . .

# Build with MLIR support (Z3/solver takes ~5-15 min on first build)
RUN cargo build --release --features mlir -p airl-driver

# Default: run the AIRL REPL
ENTRYPOINT ["cargo", "run", "--release", "--features", "mlir", "--"]
CMD ["repl"]
