#!/bin/bash
# scripts/bootstrap.sh — DEPRECATED
#
# The C codegen bootstrap pipeline (AIRL → C → cc → binary linked to libairl_rt_c.a)
# has been retired in v0.5.0. The C runtime was removed.
#
# For native binary compilation, use the Cranelift AOT path:
#   cargo run --features aot -- compile <file.airl> -o <binary>
#
# The bootstrap compiler (lexer, parser, type checker, IR compiler) still runs
# on the bytecode VM via `airl run`.

echo "ERROR: bootstrap.sh is deprecated."
echo "The C runtime (libairl_rt_c.a) has been retired."
echo ""
echo "Use the Cranelift AOT path instead:"
echo "  cargo run --features aot -- compile <file.airl> -o <binary>"
exit 1
