#!/bin/bash
# scripts/bootstrap.sh — Three-stage bootstrap for AIRL self-hosting
#
# Stage 0: Build C runtime library
# Stage 1: Use Rust toolchain to run bootstrap compiler on test program
# Stage 2: Compile the test with C runtime → standalone native binary
#
# Usage: ./scripts/bootstrap.sh [input.airl]
# Default input: a hello-world program

set -e

cd "$(dirname "$0")/.."

RUNTIME_DIR="runtime"
BOOTSTRAP_DIR="bootstrap"
BUILD_DIR="/tmp/airl-bootstrap"
mkdir -p "$BUILD_DIR"

INPUT="${1:-}"
if [ -z "$INPUT" ]; then
    # Default test program (in current directory for read-file compatibility)
    INPUT="__bootstrap_test__.airl"
    cat > "$INPUT" << 'AIRL'
(defn factorial
  :sig [(n : i64) -> i64]
  :requires [(valid n)]
  :ensures [(valid result)]
  :body (if (<= n 1) 1 (* n (factorial (- n 1)))))

(print "fact 5:" (factorial 5))
(print "fact 10:" (factorial 10))
(print "Self-hosting bootstrap works!")
AIRL
fi

echo "=== Stage 0: Build C runtime ==="
(cd "$RUNTIME_DIR" && make clean && make libairl_rt_c.a)
echo "  Built: $RUNTIME_DIR/libairl_rt_c.a"

echo ""
echo "=== Stage 1: Compile AIRL → C via bootstrap compiler ==="
# Concatenate all bootstrap modules + driver
cat "$BOOTSTRAP_DIR/lexer.airl" \
    "$BOOTSTRAP_DIR/parser.airl" \
    "$BOOTSTRAP_DIR/compiler.airl" \
    "$BOOTSTRAP_DIR/codegen_c.airl" \
    "$BOOTSTRAP_DIR/driver.airl" \
    > "$BUILD_DIR/airl_cc.airl"

echo "  Bootstrap compiler: $(wc -l < "$BUILD_DIR/airl_cc.airl") lines"
echo "  Input: $INPUT"

# Run the bootstrap compiler (uses --bytecode to avoid JIT issues with large files)
cargo run --release --features jit -- run --bytecode "$BUILD_DIR/airl_cc.airl" -- "$INPUT" 2>/dev/null \
    | sed '1s/^"//; /^nil$/d; /^"$/d' \
    > "$BUILD_DIR/output.c"

if [ ! -s "$BUILD_DIR/output.c" ]; then
    echo "ERROR: Bootstrap compiler produced no output"
    exit 1
fi

echo "  Generated: $(wc -l < "$BUILD_DIR/output.c") lines of C"

echo ""
echo "=== Stage 2: Compile C → native binary ==="
cc "$BUILD_DIR/output.c" -I"$RUNTIME_DIR" "$RUNTIME_DIR/libairl_rt_c.a" -lm -o "$BUILD_DIR/program" 2>&1
echo "  Built: $BUILD_DIR/program"
echo "  Dependencies: $(ldd "$BUILD_DIR/program" 2>/dev/null | grep -c "=>")" shared libraries

echo ""
echo "=== Stage 3: Run the native binary ==="
echo "--- output ---"
"$BUILD_DIR/program"
echo "--- end ---"

echo ""
echo "=== Bootstrap complete ==="
echo "  Source:  $INPUT"
echo "  C code:  $BUILD_DIR/output.c"
echo "  Binary:  $BUILD_DIR/program"
echo "  Runtime: $RUNTIME_DIR/libairl_rt_c.a (pure C, no Rust)"
echo ""
echo "The binary is a standalone native executable."
echo "No Rust toolchain needed to run it."

# Clean up temp test file if we created it
[ -f "__bootstrap_test__.airl" ] && rm -f "__bootstrap_test__.airl"
