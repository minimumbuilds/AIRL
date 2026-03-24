#include "airl_rt.h"
#include <math.h>
#include <time.h>
#include <sys/stat.h>

/* ---- Variants ---- */

RtValue* airl_make_variant(RtValue* tag_rv, RtValue* inner) {
    RtValue* v = malloc(sizeof(RtValue));
    v->tag = RT_VARIANT;
    v->rc = 1;
    memset(&v->data, 0, sizeof(v->data));
    /* Copy tag string */
    v->data.variant.tag_name = malloc(tag_rv->data.s.len + 1);
    memcpy(v->data.variant.tag_name, tag_rv->data.s.ptr, tag_rv->data.s.len);
    v->data.variant.tag_name[tag_rv->data.s.len] = '\0';
    /* Retain inner */
    v->data.variant.inner = inner;
    airl_value_retain(inner);
    return v;
}

RtValue* airl_match_tag(RtValue* val, RtValue* tag_rv) {
    if (val->tag != RT_VARIANT) return NULL;
    if (val->data.variant.tag_name == NULL) return NULL;
    size_t tag_len = strlen(val->data.variant.tag_name);
    if (tag_len == tag_rv->data.s.len &&
        memcmp(val->data.variant.tag_name, tag_rv->data.s.ptr, tag_len) == 0) {
        airl_value_retain(val->data.variant.inner);
        return val->data.variant.inner;
    }
    return NULL;
}

/* ---- Closures ---- */

RtValue* airl_make_closure(void* fn_ptr, RtValue** captures, size_t count) {
    RtValue* v = malloc(sizeof(RtValue));
    v->tag = RT_CLOSURE;
    v->rc = 1;
    memset(&v->data, 0, sizeof(v->data));
    v->data.closure.fn_ptr = fn_ptr;
    v->data.closure.cap_count = count;
    if (count > 0 && captures != NULL) {
        v->data.closure.captures = malloc(count * sizeof(RtValue*));
        for (size_t i = 0; i < count; i++) {
            v->data.closure.captures[i] = captures[i];
            airl_value_retain(captures[i]);
        }
    } else {
        v->data.closure.captures = NULL;
    }
    return v;
}

RtValue* airl_call_closure(RtValue* closure, RtValue** args, int64_t argc) {
    if (closure->tag != RT_CLOSURE) {
        fprintf(stderr, "airl_call_closure: not a Closure\n");
        exit(1);
    }
    void* fn = closure->data.closure.fn_ptr;
    size_t ncap = closure->data.closure.cap_count;
    RtValue** caps = closure->data.closure.captures;
    size_t total = ncap + (size_t)argc;

    /* Build combined args array: [captures..., args...] */
    RtValue* all_args[16]; /* max 16 args */
    for (size_t i = 0; i < ncap; i++) all_args[i] = caps[i];
    for (int64_t i = 0; i < argc; i++) all_args[ncap + i] = args[i];

    /* Dispatch by total arity */
    typedef RtValue* (*F0)(void);
    typedef RtValue* (*F1)(RtValue*);
    typedef RtValue* (*F2)(RtValue*, RtValue*);
    typedef RtValue* (*F3)(RtValue*, RtValue*, RtValue*);
    typedef RtValue* (*F4)(RtValue*, RtValue*, RtValue*, RtValue*);
    typedef RtValue* (*F5)(RtValue*, RtValue*, RtValue*, RtValue*, RtValue*);
    typedef RtValue* (*F6)(RtValue*, RtValue*, RtValue*, RtValue*, RtValue*, RtValue*);
    typedef RtValue* (*F7)(RtValue*, RtValue*, RtValue*, RtValue*, RtValue*, RtValue*, RtValue*);
    typedef RtValue* (*F8)(RtValue*, RtValue*, RtValue*, RtValue*, RtValue*, RtValue*, RtValue*, RtValue*);

    switch (total) {
        case 0: return ((F0)fn)();
        case 1: return ((F1)fn)(all_args[0]);
        case 2: return ((F2)fn)(all_args[0], all_args[1]);
        case 3: return ((F3)fn)(all_args[0], all_args[1], all_args[2]);
        case 4: return ((F4)fn)(all_args[0], all_args[1], all_args[2], all_args[3]);
        case 5: return ((F5)fn)(all_args[0], all_args[1], all_args[2], all_args[3], all_args[4]);
        case 6: return ((F6)fn)(all_args[0], all_args[1], all_args[2], all_args[3], all_args[4], all_args[5]);
        case 7: return ((F7)fn)(all_args[0], all_args[1], all_args[2], all_args[3], all_args[4], all_args[5], all_args[6]);
        case 8: return ((F8)fn)(all_args[0], all_args[1], all_args[2], all_args[3], all_args[4], all_args[5], all_args[6], all_args[7]);
        default:
            fprintf(stderr, "airl_call_closure: arity %zu > 8 not supported\n", total);
            exit(1);
    }
}

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

/* ---- I/O ---- */

RtValue* airl_print(RtValue* v) {
    /* Print without surrounding quotes for strings at the top level */
    if (v->tag == RT_STR) {
        fwrite(v->data.s.ptr, 1, v->data.s.len, stdout);
    } else {
        display_value(v, stdout);
    }
    printf("\n");
    fflush(stdout);
    return airl_nil();
}

RtValue* airl_print_values(RtValue** args, int64_t count) {
    for (int64_t i = 0; i < count; i++) {
        if (i > 0) printf(" ");
        RtValue* v = args[i];
        if (v->tag == RT_STR) {
            fwrite(v->data.s.ptr, 1, v->data.s.len, stdout);
        } else {
            display_value(v, stdout);
        }
    }
    printf("\n");
    fflush(stdout);
    return airl_nil();
}

RtValue* airl_type_of(RtValue* v) {
    const char* name;
    switch (v->tag) {
        case RT_INT:     name = "Int"; break;
        case RT_FLOAT:   name = "Float"; break;
        case RT_BOOL:    name = "Bool"; break;
        case RT_STR:     name = "Str"; break;
        case RT_LIST:    name = "List"; break;
        case RT_MAP:     name = "Map"; break;
        case RT_VARIANT: name = "Variant"; break;
        case RT_CLOSURE: name = "Closure"; break;
        case RT_UNIT:    name = "Unit"; break;
        case RT_NIL:     name = "Nil"; break;
        default:         name = "Unknown"; break;
    }
    return airl_str(name, strlen(name));
}

RtValue* airl_valid(RtValue* v) {
    (void)v;
    return airl_bool(1);
}

RtValue* airl_read_file(RtValue* path) {
    if (path->tag != RT_STR) {
        fprintf(stderr, "airl_read_file: expected string path\n");
        exit(1);
    }
    /* Null-terminate the path */
    char* cpath = malloc(path->data.s.len + 1);
    memcpy(cpath, path->data.s.ptr, path->data.s.len);
    cpath[path->data.s.len] = '\0';

    FILE* f = fopen(cpath, "rb");
    free(cpath);
    if (!f) {
        fprintf(stderr, "IO error: No such file or directory\n");
        exit(1);
    }
    fseek(f, 0, SEEK_END);
    long size = ftell(f);
    fseek(f, 0, SEEK_SET);
    char* buf = malloc(size + 1);
    size_t nread = fread(buf, 1, size, f);
    (void)nread;
    buf[size] = '\0';
    fclose(f);
    RtValue* result = airl_str(buf, size);
    free(buf);
    return result;
}

/* Global argc/argv for airl_get_args */
static int g_argc = 0;
static char** g_argv = NULL;

void airl_set_args(int argc, char** argv) {
    g_argc = argc;
    g_argv = argv;
}

RtValue* airl_get_args(void) {
    RtValue** items = malloc(g_argc * sizeof(RtValue*));
    for (int i = 0; i < g_argc; i++) {
        items[i] = airl_str(g_argv[i], strlen(g_argv[i]));
    }
    RtValue* list = airl_list_new(items, g_argc);
    for (int i = 0; i < g_argc; i++) {
        airl_value_release(items[i]);
    }
    free(items);
    return list;
}

/* ---- Timing ---- */

RtValue* airl_time_now(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    int64_t millis = (int64_t)ts.tv_sec * 1000 + (int64_t)ts.tv_nsec / 1000000;
    return airl_int(millis);
}

/* ---- Environment variables ---- */

RtValue* airl_getenv(RtValue* name) {
    /* Null-terminate the name */
    char* cname = malloc(name->data.s.len + 1);
    memcpy(cname, name->data.s.ptr, name->data.s.len);
    cname[name->data.s.len] = '\0';

    const char* val = getenv(cname);
    free(cname);

    if (val) {
        RtValue* tag = airl_str("Ok", 2);
        RtValue* sval = airl_str(val, strlen(val));
        RtValue* result = airl_make_variant(tag, sval);
        airl_value_release(tag);
        return result;
    } else {
        RtValue* tag = airl_str("Err", 3);
        RtValue* msg = airl_str("not set", 7);
        RtValue* result = airl_make_variant(tag, msg);
        airl_value_release(tag);
        return result;
    }
}

/* ---- File I/O (write-file, file-exists?) ---- */

RtValue* airl_write_file(RtValue* path, RtValue* content) {
    /* Null-terminate the path */
    char* cpath = malloc(path->data.s.len + 1);
    memcpy(cpath, path->data.s.ptr, path->data.s.len);
    cpath[path->data.s.len] = '\0';

    FILE* f = fopen(cpath, "wb");
    free(cpath);
    if (!f) {
        return airl_bool(0);
    }
    size_t written = fwrite(content->data.s.ptr, 1, content->data.s.len, f);
    fclose(f);
    return airl_bool(written == content->data.s.len);
}

RtValue* airl_file_exists(RtValue* path) {
    char* cpath = malloc(path->data.s.len + 1);
    memcpy(cpath, path->data.s.ptr, path->data.s.len);
    cpath[path->data.s.len] = '\0';

    struct stat st;
    int exists = (stat(cpath, &st) == 0);
    free(cpath);
    return airl_bool(exists);
}

/* ---- Contract failure ---- */

static int64_t contract_error_kind = -1;
static int64_t contract_error_fn = -1;
static int64_t contract_error_clause = -1;

int64_t airl_jit_contract_fail(int64_t kind, int64_t fn_idx, int64_t clause_idx) {
    contract_error_kind = kind;
    contract_error_fn = fn_idx;
    contract_error_clause = clause_idx;
    fprintf(stderr, "Contract violation: kind=%lld fn=%lld clause=%lld\n",
            (long long)kind, (long long)fn_idx, (long long)clause_idx);
    return 0;
}

/* ---- Flush / Error ---- */

void airl_flush_stdout(void) {
    fflush(stdout);
}

void airl_runtime_error(const char* msg, size_t len) {
    fprintf(stderr, "Runtime error: %.*s\n", (int)len, msg);
    exit(1);
}
