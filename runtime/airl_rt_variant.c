#include "airl_rt.h"
#include <math.h>

/* Stub implementations — will be replaced in later tasks */

RtValue* airl_make_variant(RtValue* tag, RtValue* inner) { (void)tag; (void)inner; fprintf(stderr, "airl_make_variant: not implemented\n"); exit(1); }
RtValue* airl_match_tag(RtValue* val, RtValue* tag) { (void)val; (void)tag; fprintf(stderr, "airl_match_tag: not implemented\n"); exit(1); }

RtValue* airl_make_closure(void* fn_ptr, RtValue** captures, size_t count) { (void)fn_ptr; (void)captures; (void)count; fprintf(stderr, "airl_make_closure: not implemented\n"); exit(1); }
RtValue* airl_call_closure(RtValue* closure, RtValue** args, int64_t argc) { (void)closure; (void)args; (void)argc; fprintf(stderr, "airl_call_closure: not implemented\n"); exit(1); }

/* ---- Arithmetic ---- */

RtValue* airl_add(RtValue* a, RtValue* b) {
    if (a->tag == RT_INT && b->tag == RT_INT) {
        return airl_int(a->data.i + b->data.i);
    }
    if (a->tag == RT_FLOAT && b->tag == RT_FLOAT) {
        return airl_float(a->data.f + b->data.f);
    }
    if (a->tag == RT_STR && b->tag == RT_STR) {
        size_t len = a->data.s.len + b->data.s.len;
        char* buf = malloc(len);
        if (!buf) { fprintf(stderr, "airl_add: out of memory\n"); exit(1); }
        memcpy(buf, a->data.s.ptr, a->data.s.len);
        memcpy(buf + a->data.s.len, b->data.s.ptr, b->data.s.len);
        RtValue* r = airl_str(buf, len);
        free(buf);
        return r;
    }
    fprintf(stderr, "airl_add: type mismatch\n");
    exit(1);
}

RtValue* airl_sub(RtValue* a, RtValue* b) {
    if (a->tag == RT_INT && b->tag == RT_INT) {
        return airl_int(a->data.i - b->data.i);
    }
    if (a->tag == RT_FLOAT && b->tag == RT_FLOAT) {
        return airl_float(a->data.f - b->data.f);
    }
    fprintf(stderr, "airl_sub: type mismatch\n");
    exit(1);
}

RtValue* airl_mul(RtValue* a, RtValue* b) {
    if (a->tag == RT_INT && b->tag == RT_INT) {
        return airl_int(a->data.i * b->data.i);
    }
    if (a->tag == RT_FLOAT && b->tag == RT_FLOAT) {
        return airl_float(a->data.f * b->data.f);
    }
    fprintf(stderr, "airl_mul: type mismatch\n");
    exit(1);
}

RtValue* airl_div(RtValue* a, RtValue* b) {
    if (a->tag == RT_INT && b->tag == RT_INT) {
        if (b->data.i == 0) {
            fprintf(stderr, "airl_div: division by zero\n");
            exit(1);
        }
        return airl_int(a->data.i / b->data.i);
    }
    if (a->tag == RT_FLOAT && b->tag == RT_FLOAT) {
        return airl_float(a->data.f / b->data.f);
    }
    fprintf(stderr, "airl_div: type mismatch\n");
    exit(1);
}

RtValue* airl_mod(RtValue* a, RtValue* b) {
    if (a->tag == RT_INT && b->tag == RT_INT) {
        if (b->data.i == 0) {
            fprintf(stderr, "airl_mod: division by zero\n");
            exit(1);
        }
        return airl_int(a->data.i % b->data.i);
    }
    if (a->tag == RT_FLOAT && b->tag == RT_FLOAT) {
        return airl_float(fmod(a->data.f, b->data.f));
    }
    fprintf(stderr, "airl_mod: type mismatch\n");
    exit(1);
}

/* ---- Comparison ---- */

RtValue* airl_eq(RtValue* a, RtValue* b) {
    if (a->tag != b->tag) {
        return airl_bool(0);
    }
    switch (a->tag) {
        case RT_INT:   return airl_bool(a->data.i == b->data.i);
        case RT_FLOAT: return airl_bool(a->data.f == b->data.f);
        case RT_BOOL:  return airl_bool(a->data.b == b->data.b);
        case RT_STR:   return airl_bool(a->data.s.len == b->data.s.len &&
                                        memcmp(a->data.s.ptr, b->data.s.ptr, a->data.s.len) == 0);
        case RT_NIL:   return airl_bool(1);
        case RT_UNIT:  return airl_bool(1);
        default:       return airl_bool(0);
    }
}

RtValue* airl_ne(RtValue* a, RtValue* b) {
    RtValue* eq = airl_eq(a, b);
    int64_t result = !eq->data.b;
    airl_value_release(eq);
    return airl_bool(result);
}

RtValue* airl_lt(RtValue* a, RtValue* b) {
    if (a->tag != b->tag) {
        fprintf(stderr, "airl_lt: type mismatch\n");
        exit(1);
    }
    switch (a->tag) {
        case RT_INT:   return airl_bool(a->data.i < b->data.i);
        case RT_FLOAT: return airl_bool(a->data.f < b->data.f);
        case RT_STR: {
            size_t min_len = a->data.s.len < b->data.s.len ? a->data.s.len : b->data.s.len;
            int cmp = memcmp(a->data.s.ptr, b->data.s.ptr, min_len);
            if (cmp != 0) return airl_bool(cmp < 0);
            return airl_bool(a->data.s.len < b->data.s.len);
        }
        default:
            fprintf(stderr, "airl_lt: incomparable type\n");
            exit(1);
    }
}

RtValue* airl_gt(RtValue* a, RtValue* b) {
    return airl_lt(b, a);
}

RtValue* airl_le(RtValue* a, RtValue* b) {
    RtValue* gt = airl_gt(a, b);
    int64_t result = !gt->data.b;
    airl_value_release(gt);
    return airl_bool(result);
}

RtValue* airl_ge(RtValue* a, RtValue* b) {
    RtValue* lt = airl_lt(a, b);
    int64_t result = !lt->data.b;
    airl_value_release(lt);
    return airl_bool(result);
}

/* ---- Logic ---- */

RtValue* airl_not(RtValue* a) {
    return airl_bool(!airl_as_bool_raw(a));
}

RtValue* airl_and(RtValue* a, RtValue* b) {
    return airl_bool(airl_as_bool_raw(a) && airl_as_bool_raw(b));
}

RtValue* airl_or(RtValue* a, RtValue* b) {
    return airl_bool(airl_as_bool_raw(a) || airl_as_bool_raw(b));
}

RtValue* airl_xor(RtValue* a, RtValue* b) {
    return airl_bool(airl_as_bool_raw(a) != airl_as_bool_raw(b));
}

/* ---- Stubs for later tasks ---- */

RtValue* airl_print(RtValue* v) { (void)v; fprintf(stderr, "airl_print: not implemented\n"); exit(1); }
RtValue* airl_print_values(RtValue** args, int64_t count) { (void)args; (void)count; fprintf(stderr, "airl_print_values: not implemented\n"); exit(1); }
RtValue* airl_type_of(RtValue* v) { (void)v; fprintf(stderr, "airl_type_of: not implemented\n"); exit(1); }
RtValue* airl_valid(RtValue* v) { (void)v; fprintf(stderr, "airl_valid: not implemented\n"); exit(1); }
RtValue* airl_read_file(RtValue* path) { (void)path; fprintf(stderr, "airl_read_file: not implemented\n"); exit(1); }
RtValue* airl_get_args(void) { fprintf(stderr, "airl_get_args: not implemented\n"); exit(1); }

int64_t airl_jit_contract_fail(int64_t kind, int64_t fn_idx, int64_t clause_idx) { (void)kind; (void)fn_idx; (void)clause_idx; fprintf(stderr, "airl_jit_contract_fail: not implemented\n"); exit(1); }
