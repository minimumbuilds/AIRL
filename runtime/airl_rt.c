#include "airl_rt.h"
#include <math.h>

/* ---- Allocator ---- */

static RtValue* rt_alloc(uint8_t tag) {
    RtValue* v = (RtValue*)malloc(sizeof(RtValue));
    if (!v) {
        fprintf(stderr, "airl_rt: out of memory\n");
        exit(1);
    }
    v->tag = tag;
    v->rc = 1;
    memset(&v->data, 0, sizeof(v->data));
    return v;
}

/* ---- Memory management ---- */

void airl_value_retain(RtValue* v) {
    if (v) {
        v->rc++;
    }
}

void airl_value_release(RtValue* v) {
    if (!v) return;
    if (v->rc > 1) {
        v->rc--;
        return;
    }
    /* rc == 1 (or 0), free children then self */
    switch (v->tag) {
        case RT_STR:
            free(v->data.s.ptr);
            break;
        case RT_LIST: {
            if (v->data.list.parent) {
                /* This is a tail-view — release the parent, don't free items */
                airl_value_release(v->data.list.parent);
            } else {
                /* We own the items array — release elements and free */
                size_t i;
                for (i = 0; i < v->data.list.len; i++) {
                    airl_value_release(v->data.list.items[v->data.list.offset + i]);
                }
                free(v->data.list.items);
            }
            break;
        }
        case RT_MAP: {
            size_t i;
            for (i = 0; i < v->data.map.capacity; i++) {
                if (v->data.map.entries[i].occupied && !v->data.map.entries[i].deleted) {
                    free(v->data.map.entries[i].key);
                    airl_value_release(v->data.map.entries[i].value);
                }
            }
            free(v->data.map.entries);
            break;
        }
        case RT_VARIANT:
            free(v->data.variant.tag_name);
            if (v->data.variant.inner) {
                airl_value_release(v->data.variant.inner);
            }
            break;
        case RT_CLOSURE: {
            size_t i;
            for (i = 0; i < v->data.closure.cap_count; i++) {
                airl_value_release(v->data.closure.captures[i]);
            }
            free(v->data.closure.captures);
            break;
        }
        default:
            break;
    }
    free(v);
}

RtValue* airl_value_clone(RtValue* v) {
    if (!v) return NULL;
    switch (v->tag) {
        case RT_NIL:
            return airl_nil();
        case RT_INT:
            return airl_int(v->data.i);
        case RT_FLOAT:
            return airl_float(v->data.f);
        case RT_BOOL:
            return airl_bool(v->data.b);
        case RT_STR:
            return airl_str(v->data.s.ptr, v->data.s.len);
        case RT_UNIT:
            return airl_unit();
        case RT_LIST: {
            size_t i;
            size_t off = v->data.list.offset;
            RtValue* clone = rt_alloc(RT_LIST);
            clone->data.list.len = v->data.list.len;
            clone->data.list.cap = v->data.list.len;
            clone->data.list.offset = 0;
            clone->data.list.parent = NULL;
            if (v->data.list.len > 0) {
                clone->data.list.items = (RtValue**)malloc(sizeof(RtValue*) * v->data.list.len);
                for (i = 0; i < v->data.list.len; i++) {
                    clone->data.list.items[i] = airl_value_clone(v->data.list.items[off + i]);
                }
            }
            return clone;
        }
        case RT_MAP: {
            size_t i;
            RtValue* clone = rt_alloc(RT_MAP);
            clone->data.map.capacity = v->data.map.capacity;
            clone->data.map.count = v->data.map.count;
            if (v->data.map.capacity > 0) {
                clone->data.map.entries = (MapEntry*)calloc(v->data.map.capacity, sizeof(MapEntry));
                for (i = 0; i < v->data.map.capacity; i++) {
                    if (v->data.map.entries[i].occupied && !v->data.map.entries[i].deleted) {
                        clone->data.map.entries[i].occupied = true;
                        clone->data.map.entries[i].key = (char*)malloc(v->data.map.entries[i].key_len + 1);
                        memcpy(clone->data.map.entries[i].key, v->data.map.entries[i].key, v->data.map.entries[i].key_len + 1);
                        clone->data.map.entries[i].key_len = v->data.map.entries[i].key_len;
                        clone->data.map.entries[i].value = airl_value_clone(v->data.map.entries[i].value);
                    }
                }
            }
            return clone;
        }
        case RT_VARIANT: {
            RtValue* clone = rt_alloc(RT_VARIANT);
            size_t tlen = strlen(v->data.variant.tag_name);
            clone->data.variant.tag_name = (char*)malloc(tlen + 1);
            memcpy(clone->data.variant.tag_name, v->data.variant.tag_name, tlen + 1);
            clone->data.variant.inner = airl_value_clone(v->data.variant.inner);
            return clone;
        }
        case RT_CLOSURE: {
            size_t i;
            RtValue* clone = rt_alloc(RT_CLOSURE);
            clone->data.closure.fn_ptr = v->data.closure.fn_ptr;
            clone->data.closure.cap_count = v->data.closure.cap_count;
            if (v->data.closure.cap_count > 0) {
                clone->data.closure.captures = (RtValue**)malloc(sizeof(RtValue*) * v->data.closure.cap_count);
                for (i = 0; i < v->data.closure.cap_count; i++) {
                    clone->data.closure.captures[i] = v->data.closure.captures[i];
                    airl_value_retain(clone->data.closure.captures[i]);
                }
            }
            return clone;
        }
        default:
            return airl_nil();
    }
}

/* ---- Constructors ---- */

RtValue* airl_int(int64_t val) {
    RtValue* v = rt_alloc(RT_INT);
    v->data.i = val;
    return v;
}

RtValue* airl_float(double val) {
    RtValue* v = rt_alloc(RT_FLOAT);
    v->data.f = val;
    return v;
}

RtValue* airl_bool(int64_t val) {
    RtValue* v = rt_alloc(RT_BOOL);
    v->data.b = val ? 1 : 0;
    return v;
}

RtValue* airl_nil(void) {
    return rt_alloc(RT_NIL);
}

RtValue* airl_unit(void) {
    return rt_alloc(RT_UNIT);
}

RtValue* airl_str(const char* ptr, size_t len) {
    RtValue* v = rt_alloc(RT_STR);
    v->data.s.ptr = (char*)malloc(len + 1);
    if (!v->data.s.ptr) {
        fprintf(stderr, "airl_rt: out of memory\n");
        exit(1);
    }
    memcpy(v->data.s.ptr, ptr, len);
    v->data.s.ptr[len] = '\0';
    v->data.s.len = len;
    return v;
}

/* ---- Logic (raw) ---- */

int64_t airl_as_bool_raw(RtValue* v) {
    if (!v) return 0;
    switch (v->tag) {
        case RT_NIL:
            return 0;
        case RT_UNIT:
            return 0;
        case RT_BOOL:
            return v->data.b ? 1 : 0;
        case RT_INT:
            return v->data.i != 0 ? 1 : 0;
        default:
            return 1;
    }
}

/* ---- Display ---- */

void display_value(RtValue* v, FILE* out) {
    if (!v) {
        fprintf(out, "nil");
        return;
    }
    switch (v->tag) {
        case RT_NIL:
            fprintf(out, "nil");
            break;
        case RT_INT:
            fprintf(out, "%lld", (long long)v->data.i);
            break;
        case RT_FLOAT: {
            double val = v->data.f;
            /* Check if whole number: no fractional part */
            if (val == floor(val) && !isinf(val) && !isnan(val)) {
                fprintf(out, "%.1f", val);
            } else {
                /*
                 * Find shortest representation that round-trips exactly.
                 * Start from precision 1 and increase until sscanf gives
                 * back the same double.
                 */
                char buf[64];
                int prec;
                for (prec = 1; prec <= 17; prec++) {
                    double rt;
                    snprintf(buf, sizeof(buf), "%.*g", prec, val);
                    sscanf(buf, "%lf", &rt);
                    if (rt == val) break;
                }
                fprintf(out, "%s", buf);
            }
            break;
        }
        case RT_BOOL:
            fprintf(out, "%s", v->data.b ? "true" : "false");
            break;
        case RT_STR:
            fprintf(out, "\"%.*s\"", (int)v->data.s.len, v->data.s.ptr);
            break;
        case RT_UNIT:
            fprintf(out, "()");
            break;
        case RT_LIST: {
            size_t i;
            size_t off = v->data.list.offset;
            fprintf(out, "[");
            for (i = 0; i < v->data.list.len; i++) {
                if (i > 0) fprintf(out, " ");
                display_value(v->data.list.items[off + i], out);
            }
            fprintf(out, "]");
            break;
        }
        case RT_VARIANT:
            fprintf(out, "(%s", v->data.variant.tag_name);
            if (v->data.variant.inner && v->data.variant.inner->tag != RT_UNIT) {
                fprintf(out, " ");
                display_value(v->data.variant.inner, out);
            }
            fprintf(out, ")");
            break;
        case RT_MAP: {
            /* Collect keys, sort them, print in order */
            size_t i, j, n = 0;
            char** keys = NULL;
            size_t* key_lens = NULL;
            RtValue** vals = NULL;

            if (v->data.map.count > 0) {
                keys = (char**)malloc(sizeof(char*) * v->data.map.count);
                key_lens = (size_t*)malloc(sizeof(size_t) * v->data.map.count);
                vals = (RtValue**)malloc(sizeof(RtValue*) * v->data.map.count);

                for (i = 0; i < v->data.map.capacity; i++) {
                    if (v->data.map.entries[i].occupied && !v->data.map.entries[i].deleted) {
                        keys[n] = v->data.map.entries[i].key;
                        key_lens[n] = v->data.map.entries[i].key_len;
                        vals[n] = v->data.map.entries[i].value;
                        n++;
                    }
                }
                /* Simple insertion sort by key */
                for (i = 1; i < n; i++) {
                    char* tk = keys[i];
                    size_t tl = key_lens[i];
                    RtValue* tv = vals[i];
                    j = i;
                    while (j > 0 && strcmp(keys[j - 1], tk) > 0) {
                        keys[j] = keys[j - 1];
                        key_lens[j] = key_lens[j - 1];
                        vals[j] = vals[j - 1];
                        j--;
                    }
                    keys[j] = tk;
                    key_lens[j] = tl;
                    vals[j] = tv;
                }
            }

            fprintf(out, "{");
            for (i = 0; i < n; i++) {
                if (i > 0) fprintf(out, " ");
                fprintf(out, "%s: ", keys[i]);
                display_value(vals[i], out);
            }
            fprintf(out, "}");

            free(keys);
            free(key_lens);
            free(vals);
            break;
        }
        case RT_CLOSURE:
            fprintf(out, "<closure>");
            break;
        default:
            fprintf(out, "<unknown>");
            break;
    }
}
