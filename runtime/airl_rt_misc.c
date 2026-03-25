/*
 * airl_rt_misc.c - Miscellaneous builtins for AOT compilation
 *
 * Implements builtins that were previously only available in the
 * bytecode VM path (Rust builtins.rs) but are needed for AOT.
 */

#define _DEFAULT_SOURCE
#include "airl_rt.h"
#include <time.h>
#include <unistd.h>
#include <regex.h>
#include <sys/stat.h>
#include <limits.h>
#include <math.h>

/* Forward declarations from other runtime files */
extern RtValue* airl_make_variant(RtValue* tag, RtValue* inner);

/* ---- Helper: Ok/Err variants ---- */

static RtValue* misc_ok(RtValue* inner) {
    RtValue* tag = airl_str("Ok", 2);
    RtValue* result = airl_make_variant(tag, inner);
    airl_value_release(tag);
    return result;
}

static RtValue* misc_err(const char* msg) {
    RtValue* tag = airl_str("Err", 3);
    RtValue* inner = airl_str(msg, strlen(msg));
    RtValue* result = airl_make_variant(tag, inner);
    airl_value_release(tag);
    return result;
}

/* ---- char-count: Unicode character count ---- */

RtValue* airl_char_count(RtValue* s) {
    if (s->tag != RT_STR) return airl_int(0);
    const char* p = s->data.s.ptr;
    size_t len = s->data.s.len;
    size_t count = 0;
    size_t i = 0;
    while (i < len) {
        unsigned char c = (unsigned char)p[i];
        if (c < 0x80) i += 1;
        else if (c < 0xE0) i += 2;
        else if (c < 0xF0) i += 3;
        else i += 4;
        count++;
    }
    return airl_int((int64_t)count);
}

/* ---- str: variadic string concatenation ---- */

RtValue* airl_str_variadic(RtValue** args, int64_t argc) {
    size_t total = 0;
    char** parts = (char**)malloc(argc * sizeof(char*));
    size_t* part_lens = (size_t*)malloc(argc * sizeof(size_t));
    int* needs_free = (int*)calloc(argc, sizeof(int));

    for (int64_t i = 0; i < argc; i++) {
        RtValue* v = args[i];
        if (v->tag == RT_STR) {
            parts[i] = v->data.s.ptr;
            part_lens[i] = v->data.s.len;
        } else if (v->tag == RT_INT) {
            char buf[32];
            int n = snprintf(buf, sizeof(buf), "%ld", (long)v->data.i);
            parts[i] = (char*)malloc(n + 1);
            memcpy(parts[i], buf, n + 1);
            part_lens[i] = n;
            needs_free[i] = 1;
        } else if (v->tag == RT_FLOAT) {
            char buf[64];
            int n = snprintf(buf, sizeof(buf), "%g", v->data.f);
            parts[i] = (char*)malloc(n + 1);
            memcpy(parts[i], buf, n + 1);
            part_lens[i] = n;
            needs_free[i] = 1;
        } else if (v->tag == RT_BOOL) {
            const char* b = v->data.b ? "true" : "false";
            parts[i] = (char*)b;
            part_lens[i] = strlen(b);
        } else if (v->tag == RT_NIL) {
            parts[i] = (char*)"nil";
            part_lens[i] = 3;
        } else {
            parts[i] = (char*)"<value>";
            part_lens[i] = 7;
        }
        total += part_lens[i];
    }

    char* result = (char*)malloc(total + 1);
    size_t pos = 0;
    for (int64_t i = 0; i < argc; i++) {
        memcpy(result + pos, parts[i], part_lens[i]);
        if (needs_free[i]) free(parts[i]);
        pos += part_lens[i];
    }
    result[pos] = '\0';

    RtValue* rv = airl_str(result, total);
    free(result);
    free(parts);
    free(part_lens);
    free(needs_free);
    return rv;
}

/* ---- format: template string with {} placeholders ---- */

RtValue* airl_format_variadic(RtValue** args, int64_t argc) {
    if (argc < 1 || args[0]->tag != RT_STR) return airl_str("", 0);

    const char* tmpl = args[0]->data.s.ptr;
    size_t tmpl_len = args[0]->data.s.len;

    size_t out_cap = tmpl_len + argc * 32;
    char* out = (char*)malloc(out_cap);
    size_t out_len = 0;
    int64_t arg_idx = 1;

    for (size_t i = 0; i < tmpl_len; i++) {
        if (tmpl[i] == '{' && i + 1 < tmpl_len && tmpl[i + 1] == '}' && arg_idx < argc) {
            RtValue* v = args[arg_idx++];
            char buf[128];
            int n = 0;
            if (v->tag == RT_STR) {
                while (out_len + v->data.s.len >= out_cap) { out_cap *= 2; out = realloc(out, out_cap); }
                memcpy(out + out_len, v->data.s.ptr, v->data.s.len);
                out_len += v->data.s.len;
                i++;
                continue;
            } else if (v->tag == RT_INT) {
                n = snprintf(buf, sizeof(buf), "%ld", (long)v->data.i);
            } else if (v->tag == RT_FLOAT) {
                n = snprintf(buf, sizeof(buf), "%g", v->data.f);
            } else if (v->tag == RT_BOOL) {
                n = snprintf(buf, sizeof(buf), "%s", v->data.b ? "true" : "false");
            } else {
                n = snprintf(buf, sizeof(buf), "<value>");
            }
            while (out_len + n >= out_cap) { out_cap *= 2; out = realloc(out, out_cap); }
            memcpy(out + out_len, buf, n);
            out_len += n;
            i++;
        } else {
            if (out_len + 1 >= out_cap) { out_cap *= 2; out = realloc(out, out_cap); }
            out[out_len++] = tmpl[i];
        }
    }

    RtValue* rv = airl_str(out, out_len);
    free(out);
    return rv;
}

/* ---- assert(condition, msg) ---- */

RtValue* airl_assert(RtValue* cond, RtValue* msg) {
    int truth = 0;
    if (cond->tag == RT_BOOL) truth = cond->data.b != 0;
    else if (cond->tag == RT_INT) truth = cond->data.i != 0;
    else truth = (cond->tag != RT_NIL);

    if (!truth) {
        if (msg->tag == RT_STR) {
            fprintf(stderr, "Assertion failed: %.*s\n", (int)msg->data.s.len, msg->data.s.ptr);
        } else {
            fprintf(stderr, "Assertion failed\n");
        }
        fflush(stderr);
        _exit(1);
    }
    return airl_bool(1);
}

/* ---- panic(msg) ---- */

RtValue* airl_panic(RtValue* msg) {
    if (msg->tag == RT_STR) {
        fprintf(stderr, "panic: %.*s\n", (int)msg->data.s.len, msg->data.s.ptr);
    } else {
        fprintf(stderr, "panic\n");
    }
    fflush(stderr);
    _exit(1);
    return airl_nil();
}

/* ---- exit(code) ---- */

RtValue* airl_exit(RtValue* code) {
    int c = (code->tag == RT_INT) ? (int)code->data.i : 1;
    _exit(c);
    return airl_nil();
}

/* ---- sleep(ms) ---- */

RtValue* airl_sleep(RtValue* ms) {
    int64_t millis = ms->data.i;
    if (millis > 0) {
        usleep((useconds_t)(millis * 1000));
    }
    return airl_nil();
}

/* ---- format-time(ms, fmt) ---- */

RtValue* airl_format_time(RtValue* ms_val, RtValue* fmt_val) {
    int64_t ms = ms_val->data.i;
    time_t secs = (time_t)(ms / 1000);
    struct tm utc;
    gmtime_r(&secs, &utc);

    char* fmt = (char*)malloc(fmt_val->data.s.len + 1);
    memcpy(fmt, fmt_val->data.s.ptr, fmt_val->data.s.len);
    fmt[fmt_val->data.s.len] = '\0';

    char buf[256];
    strftime(buf, sizeof(buf), fmt, &utc);
    free(fmt);

    return airl_str(buf, strlen(buf));
}

/* ---- read-lines(path) -> List[Str] ---- */

RtValue* airl_read_lines(RtValue* path) {
    char* p = (char*)malloc(path->data.s.len + 1);
    memcpy(p, path->data.s.ptr, path->data.s.len);
    p[path->data.s.len] = '\0';

    FILE* f = fopen(p, "r");
    free(p);
    if (!f) return airl_list_new(NULL, 0);

    size_t cap = 16, count = 0;
    RtValue** items = (RtValue**)malloc(cap * sizeof(RtValue*));

    char* line = NULL;
    size_t line_cap = 0;
    ssize_t n;
    while ((n = getline(&line, &line_cap, f)) != -1) {
        while (n > 0 && (line[n-1] == '\n' || line[n-1] == '\r')) n--;
        if (count >= cap) { cap *= 2; items = realloc(items, cap * sizeof(RtValue*)); }
        items[count++] = airl_str(line, (size_t)n);
    }
    free(line);
    fclose(f);

    RtValue* list = airl_list_new(items, count);
    for (size_t i = 0; i < count; i++) airl_value_release(items[i]);
    free(items);
    return list;
}

/* ---- List operations ---- */

RtValue* airl_concat_lists(RtValue* a, RtValue* b) {
    size_t a_len = a->data.list.len, b_len = b->data.list.len;
    size_t a_off = a->data.list.offset, b_off = b->data.list.offset;
    size_t total = a_len + b_len;
    RtValue** items = (RtValue**)malloc(total * sizeof(RtValue*));
    for (size_t i = 0; i < a_len; i++) items[i] = a->data.list.items[a_off + i];
    for (size_t i = 0; i < b_len; i++) items[a_len + i] = b->data.list.items[b_off + i];
    RtValue* result = airl_list_new(items, total);
    free(items);
    return result;
}

RtValue* airl_range(RtValue* start, RtValue* end) {
    int64_t s = start->data.i, e = end->data.i;
    if (s >= e) return airl_list_new(NULL, 0);
    size_t count = (size_t)(e - s);
    RtValue** items = (RtValue**)malloc(count * sizeof(RtValue*));
    for (size_t i = 0; i < count; i++) items[i] = airl_int(s + (int64_t)i);
    RtValue* result = airl_list_new(items, count);
    for (size_t i = 0; i < count; i++) airl_value_release(items[i]);
    free(items);
    return result;
}

RtValue* airl_reverse_list(RtValue* list) {
    size_t len = list->data.list.len;
    size_t off = list->data.list.offset;
    RtValue** items = (RtValue**)malloc(len * sizeof(RtValue*));
    for (size_t i = 0; i < len; i++) items[i] = list->data.list.items[off + len - 1 - i];
    RtValue* result = airl_list_new(items, len);
    free(items);
    return result;
}

RtValue* airl_take(RtValue* n_val, RtValue* list) {
    size_t n = (size_t)n_val->data.i;
    size_t len = list->data.list.len;
    size_t off = list->data.list.offset;
    if (n > len) n = len;
    RtValue** items = (RtValue**)malloc(n * sizeof(RtValue*));
    for (size_t i = 0; i < n; i++) items[i] = list->data.list.items[off + i];
    RtValue* result = airl_list_new(items, n);
    free(items);
    return result;
}

RtValue* airl_drop(RtValue* n_val, RtValue* list) {
    size_t n = (size_t)n_val->data.i;
    size_t len = list->data.list.len;
    size_t off = list->data.list.offset;
    if (n >= len) return airl_list_new(NULL, 0);
    size_t new_len = len - n;
    RtValue** items = (RtValue**)malloc(new_len * sizeof(RtValue*));
    for (size_t i = 0; i < new_len; i++) items[i] = list->data.list.items[off + n + i];
    RtValue* result = airl_list_new(items, new_len);
    free(items);
    return result;
}

RtValue* airl_zip(RtValue* a, RtValue* b) {
    size_t a_len = a->data.list.len, b_len = b->data.list.len;
    size_t a_off = a->data.list.offset, b_off = b->data.list.offset;
    size_t len = a_len < b_len ? a_len : b_len;
    RtValue** items = (RtValue**)malloc(len * sizeof(RtValue*));
    for (size_t i = 0; i < len; i++) {
        RtValue* pair[2] = { a->data.list.items[a_off + i], b->data.list.items[b_off + i] };
        items[i] = airl_list_new(pair, 2);
    }
    RtValue* result = airl_list_new(items, len);
    for (size_t i = 0; i < len; i++) airl_value_release(items[i]);
    free(items);
    return result;
}

RtValue* airl_flatten(RtValue* list) {
    size_t len = list->data.list.len;
    size_t off = list->data.list.offset;
    size_t total = 0;
    for (size_t i = 0; i < len; i++) {
        RtValue* sub = list->data.list.items[off + i];
        if (sub->tag == RT_LIST) total += sub->data.list.len;
        else total++;
    }
    RtValue** items = (RtValue**)malloc(total * sizeof(RtValue*));
    size_t pos = 0;
    for (size_t i = 0; i < len; i++) {
        RtValue* sub = list->data.list.items[off + i];
        if (sub->tag == RT_LIST) {
            for (size_t j = 0; j < sub->data.list.len; j++)
                items[pos++] = sub->data.list.items[sub->data.list.offset + j];
        } else {
            items[pos++] = sub;
        }
    }
    RtValue* result = airl_list_new(items, total);
    free(items);
    return result;
}

RtValue* airl_enumerate(RtValue* list) {
    size_t len = list->data.list.len;
    size_t off = list->data.list.offset;
    RtValue** items = (RtValue**)malloc(len * sizeof(RtValue*));
    for (size_t i = 0; i < len; i++) {
        RtValue* idx = airl_int((int64_t)i);
        RtValue* pair[2] = { idx, list->data.list.items[off + i] };
        items[i] = airl_list_new(pair, 2);
        airl_value_release(idx);
    }
    RtValue* result = airl_list_new(items, len);
    for (size_t i = 0; i < len; i++) airl_value_release(items[i]);
    free(items);
    return result;
}

/* ---- Path operations ---- */

RtValue* airl_path_join(RtValue* parts) {
    if (parts->tag != RT_LIST || parts->data.list.len == 0) return airl_str("", 0);
    size_t len = parts->data.list.len;
    size_t off = parts->data.list.offset;
    size_t total = 0;
    for (size_t i = 0; i < len; i++) {
        RtValue* p = parts->data.list.items[off + i];
        if (p->tag == RT_STR) total += p->data.s.len + 1;
    }
    char* buf = (char*)malloc(total + 1);
    size_t pos = 0;
    for (size_t i = 0; i < len; i++) {
        RtValue* p = parts->data.list.items[off + i];
        if (p->tag == RT_STR) {
            if (pos > 0 && buf[pos-1] != '/') buf[pos++] = '/';
            memcpy(buf + pos, p->data.s.ptr, p->data.s.len);
            pos += p->data.s.len;
        }
    }
    RtValue* result = airl_str(buf, pos);
    free(buf);
    return result;
}

RtValue* airl_path_parent(RtValue* path) {
    const char* p = path->data.s.ptr;
    size_t len = path->data.s.len;
    while (len > 0 && p[len-1] != '/') len--;
    if (len > 1) len--;
    return airl_str(p, len);
}

RtValue* airl_path_filename(RtValue* path) {
    const char* p = path->data.s.ptr;
    size_t len = path->data.s.len;
    size_t start = len;
    while (start > 0 && p[start-1] != '/') start--;
    return airl_str(p + start, len - start);
}

RtValue* airl_path_extension(RtValue* path) {
    const char* p = path->data.s.ptr;
    size_t len = path->data.s.len;
    size_t dot = len;
    while (dot > 0 && p[dot-1] != '.' && p[dot-1] != '/') dot--;
    if (dot > 0 && p[dot-1] == '.') return airl_str(p + dot, len - dot);
    return airl_str("", 0);
}

RtValue* airl_is_absolute(RtValue* path) {
    if (path->data.s.len > 0 && path->data.s.ptr[0] == '/') return airl_bool(1);
    return airl_bool(0);
}

/* ---- Regex operations (POSIX regex) ---- */

RtValue* airl_regex_match(RtValue* pat, RtValue* s) {
    char* pattern = (char*)malloc(pat->data.s.len + 1);
    memcpy(pattern, pat->data.s.ptr, pat->data.s.len);
    pattern[pat->data.s.len] = '\0';
    char* str = (char*)malloc(s->data.s.len + 1);
    memcpy(str, s->data.s.ptr, s->data.s.len);
    str[s->data.s.len] = '\0';

    regex_t re;
    if (regcomp(&re, pattern, REG_EXTENDED) != 0) { free(pattern); free(str); return airl_nil(); }
    regmatch_t match;
    RtValue* result;
    if (regexec(&re, str, 1, &match, 0) == 0)
        result = airl_str(str + match.rm_so, match.rm_eo - match.rm_so);
    else
        result = airl_nil();
    regfree(&re); free(pattern); free(str);
    return result;
}

RtValue* airl_regex_find_all(RtValue* pat, RtValue* s) {
    char* pattern = (char*)malloc(pat->data.s.len + 1);
    memcpy(pattern, pat->data.s.ptr, pat->data.s.len);
    pattern[pat->data.s.len] = '\0';
    char* str = (char*)malloc(s->data.s.len + 1);
    memcpy(str, s->data.s.ptr, s->data.s.len);
    str[s->data.s.len] = '\0';

    regex_t re;
    if (regcomp(&re, pattern, REG_EXTENDED) != 0) { free(pattern); free(str); return airl_list_new(NULL, 0); }

    size_t cap = 16, count = 0;
    RtValue** items = (RtValue**)malloc(cap * sizeof(RtValue*));
    regmatch_t match;
    const char* cursor = str;
    while (regexec(&re, cursor, 1, &match, 0) == 0) {
        if (count >= cap) { cap *= 2; items = realloc(items, cap * sizeof(RtValue*)); }
        items[count++] = airl_str(cursor + match.rm_so, match.rm_eo - match.rm_so);
        cursor += match.rm_eo;
        if (match.rm_so == match.rm_eo) { if (*cursor) cursor++; else break; }
    }
    regfree(&re); free(pattern); free(str);
    RtValue* result = airl_list_new(items, count);
    for (size_t i = 0; i < count; i++) airl_value_release(items[i]);
    free(items);
    return result;
}

RtValue* airl_regex_replace(RtValue* pat, RtValue* s, RtValue* replacement) {
    char* pattern = (char*)malloc(pat->data.s.len + 1);
    memcpy(pattern, pat->data.s.ptr, pat->data.s.len);
    pattern[pat->data.s.len] = '\0';
    char* str = (char*)malloc(s->data.s.len + 1);
    memcpy(str, s->data.s.ptr, s->data.s.len);
    str[s->data.s.len] = '\0';

    regex_t re;
    if (regcomp(&re, pattern, REG_EXTENDED) != 0) {
        free(pattern); RtValue* r = airl_str(str, s->data.s.len); free(str); return r;
    }

    size_t out_cap = s->data.s.len + replacement->data.s.len * 4 + 64;
    char* out = (char*)malloc(out_cap);
    size_t out_len = 0;
    regmatch_t match;
    const char* cursor = str;
    while (regexec(&re, cursor, 1, &match, 0) == 0) {
        while (out_len + match.rm_so + replacement->data.s.len >= out_cap) { out_cap *= 2; out = realloc(out, out_cap); }
        memcpy(out + out_len, cursor, match.rm_so);
        out_len += match.rm_so;
        memcpy(out + out_len, replacement->data.s.ptr, replacement->data.s.len);
        out_len += replacement->data.s.len;
        cursor += match.rm_eo;
        if (match.rm_so == match.rm_eo) { if (*cursor) { out[out_len++] = *cursor++; } else break; }
    }
    size_t remaining = strlen(cursor);
    while (out_len + remaining >= out_cap) { out_cap *= 2; out = realloc(out, out_cap); }
    memcpy(out + out_len, cursor, remaining);
    out_len += remaining;
    regfree(&re); free(pattern); free(str);
    RtValue* result = airl_str(out, out_len);
    free(out);
    return result;
}

RtValue* airl_regex_split(RtValue* pat, RtValue* s) {
    char* pattern = (char*)malloc(pat->data.s.len + 1);
    memcpy(pattern, pat->data.s.ptr, pat->data.s.len);
    pattern[pat->data.s.len] = '\0';
    char* str = (char*)malloc(s->data.s.len + 1);
    memcpy(str, s->data.s.ptr, s->data.s.len);
    str[s->data.s.len] = '\0';

    regex_t re;
    if (regcomp(&re, pattern, REG_EXTENDED) != 0) {
        free(pattern);
        RtValue** items = (RtValue**)malloc(sizeof(RtValue*));
        items[0] = airl_str(str, s->data.s.len); free(str);
        RtValue* result = airl_list_new(items, 1);
        airl_value_release(items[0]); free(items);
        return result;
    }
    size_t cap = 16, count = 0;
    RtValue** items = (RtValue**)malloc(cap * sizeof(RtValue*));
    regmatch_t match;
    const char* cursor = str;
    while (regexec(&re, cursor, 1, &match, 0) == 0 && match.rm_so < match.rm_eo) {
        if (count >= cap) { cap *= 2; items = realloc(items, cap * sizeof(RtValue*)); }
        items[count++] = airl_str(cursor, match.rm_so);
        cursor += match.rm_eo;
    }
    if (count >= cap) { cap *= 2; items = realloc(items, cap * sizeof(RtValue*)); }
    items[count++] = airl_str(cursor, strlen(cursor));
    regfree(&re); free(pattern); free(str);
    RtValue* result = airl_list_new(items, count);
    for (size_t i = 0; i < count; i++) airl_value_release(items[i]);
    free(items);
    return result;
}

/* ---- Crypto stubs ---- */

RtValue* airl_sha256(RtValue* s) {
    (void)s;
    return airl_str("<sha256-not-available-in-aot>", 28);
}

RtValue* airl_hmac_sha256(RtValue* key, RtValue* msg) {
    (void)key; (void)msg;
    return airl_str("<hmac-sha256-not-available-in-aot>", 34);
}

RtValue* airl_base64_encode(RtValue* s) {
    static const char b64[] = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    const unsigned char* in = (const unsigned char*)s->data.s.ptr;
    size_t in_len = s->data.s.len;
    size_t out_len = 4 * ((in_len + 2) / 3);
    char* out = (char*)malloc(out_len + 1);
    size_t j = 0;
    for (size_t i = 0; i < in_len; i += 3) {
        uint32_t n = ((uint32_t)in[i]) << 16;
        if (i + 1 < in_len) n |= ((uint32_t)in[i+1]) << 8;
        if (i + 2 < in_len) n |= ((uint32_t)in[i+2]);
        out[j++] = b64[(n >> 18) & 0x3F];
        out[j++] = b64[(n >> 12) & 0x3F];
        out[j++] = (i + 1 < in_len) ? b64[(n >> 6) & 0x3F] : '=';
        out[j++] = (i + 2 < in_len) ? b64[n & 0x3F] : '=';
    }
    RtValue* result = airl_str(out, j);
    free(out);
    return result;
}

RtValue* airl_base64_decode(RtValue* s) {
    static const unsigned char d[] = {
        ['A']=0,['B']=1,['C']=2,['D']=3,['E']=4,['F']=5,['G']=6,['H']=7,
        ['I']=8,['J']=9,['K']=10,['L']=11,['M']=12,['N']=13,['O']=14,['P']=15,
        ['Q']=16,['R']=17,['S']=18,['T']=19,['U']=20,['V']=21,['W']=22,['X']=23,
        ['Y']=24,['Z']=25,['a']=26,['b']=27,['c']=28,['d']=29,['e']=30,['f']=31,
        ['g']=32,['h']=33,['i']=34,['j']=35,['k']=36,['l']=37,['m']=38,['n']=39,
        ['o']=40,['p']=41,['q']=42,['r']=43,['s']=44,['t']=45,['u']=46,['v']=47,
        ['w']=48,['x']=49,['y']=50,['z']=51,['0']=52,['1']=53,['2']=54,['3']=55,
        ['4']=56,['5']=57,['6']=58,['7']=59,['8']=60,['9']=61,['+']=62,['/']=63,
    };
    const unsigned char* in = (const unsigned char*)s->data.s.ptr;
    size_t in_len = s->data.s.len;
    size_t out_cap = in_len * 3 / 4 + 4;
    char* out = (char*)malloc(out_cap);
    size_t j = 0;
    for (size_t i = 0; i + 3 < in_len; i += 4) {
        uint32_t n = (d[in[i]] << 18) | (d[in[i+1]] << 12) | (d[in[i+2]] << 6) | d[in[i+3]];
        out[j++] = (n >> 16) & 0xFF;
        if (in[i+2] != '=') out[j++] = (n >> 8) & 0xFF;
        if (in[i+3] != '=') out[j++] = n & 0xFF;
    }
    RtValue* result = airl_str(out, j);
    free(out);
    return result;
}

RtValue* airl_random_bytes(RtValue* n) {
    int64_t count = n->data.i;
    char* buf = (char*)malloc(count * 2 + 1);
    FILE* f = fopen("/dev/urandom", "rb");
    size_t pos = 0;
    if (f) {
        for (int64_t i = 0; i < count; i++) {
            unsigned char b;
            if (fread(&b, 1, 1, f) == 1) {
                sprintf(buf + pos, "%02x", b);
                pos += 2;
            }
        }
        fclose(f);
    }
    RtValue* result = airl_str(buf, pos);
    free(buf);
    return result;
}

/* ---- string-to-float ---- */

RtValue* airl_string_to_float(RtValue* s) {
    char* str = (char*)malloc(s->data.s.len + 1);
    memcpy(str, s->data.s.ptr, s->data.s.len);
    str[s->data.s.len] = '\0';
    char* end;
    double val = strtod(str, &end);
    RtValue* result;
    if (end != str && *end == '\0') {
        result = misc_ok(airl_float(val));
    } else {
        result = misc_err("invalid float");
    }
    free(str);
    return result;
}
