#include "airl_rt.h"

/* Stub implementations — will be replaced in later tasks */

RtValue* airl_make_variant(RtValue* tag, RtValue* inner) { (void)tag; (void)inner; fprintf(stderr, "airl_make_variant: not implemented\n"); exit(1); }
RtValue* airl_match_tag(RtValue* val, RtValue* tag) { (void)val; (void)tag; fprintf(stderr, "airl_match_tag: not implemented\n"); exit(1); }

RtValue* airl_make_closure(void* fn_ptr, RtValue** captures, size_t count) { (void)fn_ptr; (void)captures; (void)count; fprintf(stderr, "airl_make_closure: not implemented\n"); exit(1); }
RtValue* airl_call_closure(RtValue* closure, RtValue** args, int64_t argc) { (void)closure; (void)args; (void)argc; fprintf(stderr, "airl_call_closure: not implemented\n"); exit(1); }

RtValue* airl_add(RtValue* a, RtValue* b) { (void)a; (void)b; fprintf(stderr, "airl_add: not implemented\n"); exit(1); }
RtValue* airl_sub(RtValue* a, RtValue* b) { (void)a; (void)b; fprintf(stderr, "airl_sub: not implemented\n"); exit(1); }
RtValue* airl_mul(RtValue* a, RtValue* b) { (void)a; (void)b; fprintf(stderr, "airl_mul: not implemented\n"); exit(1); }
RtValue* airl_div(RtValue* a, RtValue* b) { (void)a; (void)b; fprintf(stderr, "airl_div: not implemented\n"); exit(1); }
RtValue* airl_mod(RtValue* a, RtValue* b) { (void)a; (void)b; fprintf(stderr, "airl_mod: not implemented\n"); exit(1); }

RtValue* airl_eq(RtValue* a, RtValue* b) { (void)a; (void)b; fprintf(stderr, "airl_eq: not implemented\n"); exit(1); }
RtValue* airl_ne(RtValue* a, RtValue* b) { (void)a; (void)b; fprintf(stderr, "airl_ne: not implemented\n"); exit(1); }
RtValue* airl_lt(RtValue* a, RtValue* b) { (void)a; (void)b; fprintf(stderr, "airl_lt: not implemented\n"); exit(1); }
RtValue* airl_gt(RtValue* a, RtValue* b) { (void)a; (void)b; fprintf(stderr, "airl_gt: not implemented\n"); exit(1); }
RtValue* airl_le(RtValue* a, RtValue* b) { (void)a; (void)b; fprintf(stderr, "airl_le: not implemented\n"); exit(1); }
RtValue* airl_ge(RtValue* a, RtValue* b) { (void)a; (void)b; fprintf(stderr, "airl_ge: not implemented\n"); exit(1); }

RtValue* airl_not(RtValue* a) { (void)a; fprintf(stderr, "airl_not: not implemented\n"); exit(1); }
RtValue* airl_and(RtValue* a, RtValue* b) { (void)a; (void)b; fprintf(stderr, "airl_and: not implemented\n"); exit(1); }
RtValue* airl_or(RtValue* a, RtValue* b) { (void)a; (void)b; fprintf(stderr, "airl_or: not implemented\n"); exit(1); }
RtValue* airl_xor(RtValue* a, RtValue* b) { (void)a; (void)b; fprintf(stderr, "airl_xor: not implemented\n"); exit(1); }

RtValue* airl_print(RtValue* v) { (void)v; fprintf(stderr, "airl_print: not implemented\n"); exit(1); }
RtValue* airl_print_values(RtValue** args, int64_t count) { (void)args; (void)count; fprintf(stderr, "airl_print_values: not implemented\n"); exit(1); }
RtValue* airl_type_of(RtValue* v) { (void)v; fprintf(stderr, "airl_type_of: not implemented\n"); exit(1); }
RtValue* airl_valid(RtValue* v) { (void)v; fprintf(stderr, "airl_valid: not implemented\n"); exit(1); }
RtValue* airl_read_file(RtValue* path) { (void)path; fprintf(stderr, "airl_read_file: not implemented\n"); exit(1); }
RtValue* airl_get_args(void) { fprintf(stderr, "airl_get_args: not implemented\n"); exit(1); }

int64_t airl_jit_contract_fail(int64_t kind, int64_t fn_idx, int64_t clause_idx) { (void)kind; (void)fn_idx; (void)clause_idx; fprintf(stderr, "airl_jit_contract_fail: not implemented\n"); exit(1); }
