/*
 * airl_rt_json.c - JSON parse/stringify for the AIRL runtime
 *
 * Implements recursive-descent JSON parsing and value stringification.
 * No external dependencies beyond the AIRL runtime and libc.
 */

#include "airl_rt.h"
#include <ctype.h>
#include <errno.h>

/* ------------------------------------------------------------------ */
/*  Dynamic buffer for building output strings                        */
/* ------------------------------------------------------------------ */

typedef struct {
    char  *data;
    size_t len;
    size_t cap;
} JsonBuf;

static void jbuf_init(JsonBuf *b) {
    b->cap  = 128;
    b->len  = 0;
    b->data = (char *)malloc(b->cap);
}

static void jbuf_grow(JsonBuf *b, size_t need) {
    if (b->len + need <= b->cap) return;
    while (b->len + need > b->cap) b->cap *= 2;
    b->data = (char *)realloc(b->data, b->cap);
}

static void jbuf_push(JsonBuf *b, char c) {
    jbuf_grow(b, 1);
    b->data[b->len++] = c;
}

static void jbuf_append(JsonBuf *b, const char *s, size_t n) {
    jbuf_grow(b, n);
    memcpy(b->data + b->len, s, n);
    b->len += n;
}

static void jbuf_append_cstr(JsonBuf *b, const char *s) {
    jbuf_append(b, s, strlen(s));
}

/* ------------------------------------------------------------------ */
/*  Helper: wrap a value in Ok(...) or Err(...)                       */
/* ------------------------------------------------------------------ */

static RtValue *make_ok(RtValue *inner) {
    RtValue *tag = airl_str("Ok", 2);
    RtValue *result = airl_make_variant(tag, inner);
    airl_value_release(tag);
    return result;
}

static RtValue *make_err(const char *msg) {
    RtValue *tag = airl_str("Err", 3);
    RtValue *inner = airl_str(msg, strlen(msg));
    RtValue *result = airl_make_variant(tag, inner);
    airl_value_release(tag);
    return result;
}

/* ------------------------------------------------------------------ */
/*  JSON Parser (recursive descent)                                   */
/* ------------------------------------------------------------------ */

typedef struct {
    const char *src;
    size_t      len;
    size_t      pos;
    const char *error;
} JsonParser;

static void jp_skip_ws(JsonParser *p) {
    while (p->pos < p->len) {
        char c = p->src[p->pos];
        if (c == ' ' || c == '\t' || c == '\n' || c == '\r')
            p->pos++;
        else
            break;
    }
}

static char jp_peek(JsonParser *p) {
    jp_skip_ws(p);
    if (p->pos >= p->len) return '\0';
    return p->src[p->pos];
}

static char jp_next(JsonParser *p) {
    jp_skip_ws(p);
    if (p->pos >= p->len) return '\0';
    return p->src[p->pos++];
}

static int jp_expect(JsonParser *p, char c) {
    char got = jp_next(p);
    if (got != c) {
        p->error = "unexpected character";
        return 0;
    }
    return 1;
}

/* Forward declaration */
static RtValue *jp_parse_value(JsonParser *p);

static RtValue *jp_parse_string(JsonParser *p) {
    /* Opening quote already consumed or expected */
    if (!jp_expect(p, '"')) return NULL;

    JsonBuf buf;
    jbuf_init(&buf);

    while (p->pos < p->len) {
        char c = p->src[p->pos++];
        if (c == '"') {
            RtValue *result = airl_str(buf.data, buf.len);
            free(buf.data);
            return result;
        }
        if (c == '\\') {
            if (p->pos >= p->len) { p->error = "unterminated escape"; free(buf.data); return NULL; }
            char esc = p->src[p->pos++];
            switch (esc) {
                case '"':  jbuf_push(&buf, '"');  break;
                case '\\': jbuf_push(&buf, '\\'); break;
                case '/':  jbuf_push(&buf, '/');  break;
                case 'b':  jbuf_push(&buf, '\b'); break;
                case 'f':  jbuf_push(&buf, '\f'); break;
                case 'n':  jbuf_push(&buf, '\n'); break;
                case 'r':  jbuf_push(&buf, '\r'); break;
                case 't':  jbuf_push(&buf, '\t'); break;
                case 'u': {
                    /* Basic \uXXXX - just pass through as UTF-8 for ASCII range */
                    if (p->pos + 4 > p->len) { p->error = "incomplete \\u escape"; free(buf.data); return NULL; }
                    char hex[5];
                    memcpy(hex, p->src + p->pos, 4);
                    hex[4] = '\0';
                    p->pos += 4;
                    unsigned long cp = strtoul(hex, NULL, 16);
                    if (cp < 0x80) {
                        jbuf_push(&buf, (char)cp);
                    } else if (cp < 0x800) {
                        jbuf_push(&buf, (char)(0xC0 | (cp >> 6)));
                        jbuf_push(&buf, (char)(0x80 | (cp & 0x3F)));
                    } else {
                        jbuf_push(&buf, (char)(0xE0 | (cp >> 12)));
                        jbuf_push(&buf, (char)(0x80 | ((cp >> 6) & 0x3F)));
                        jbuf_push(&buf, (char)(0x80 | (cp & 0x3F)));
                    }
                    break;
                }
                default:
                    jbuf_push(&buf, esc);
                    break;
            }
        } else {
            jbuf_push(&buf, c);
        }
    }
    p->error = "unterminated string";
    free(buf.data);
    return NULL;
}

static RtValue *jp_parse_number(JsonParser *p) {
    size_t start = p->pos;
    int is_float = 0;

    if (p->pos < p->len && p->src[p->pos] == '-') p->pos++;
    while (p->pos < p->len && isdigit((unsigned char)p->src[p->pos])) p->pos++;
    if (p->pos < p->len && p->src[p->pos] == '.') {
        is_float = 1;
        p->pos++;
        while (p->pos < p->len && isdigit((unsigned char)p->src[p->pos])) p->pos++;
    }
    if (p->pos < p->len && (p->src[p->pos] == 'e' || p->src[p->pos] == 'E')) {
        is_float = 1;
        p->pos++;
        if (p->pos < p->len && (p->src[p->pos] == '+' || p->src[p->pos] == '-')) p->pos++;
        while (p->pos < p->len && isdigit((unsigned char)p->src[p->pos])) p->pos++;
    }

    size_t numlen = p->pos - start;
    char *numstr = malloc(numlen + 1);
    memcpy(numstr, p->src + start, numlen);
    numstr[numlen] = '\0';

    RtValue *result;
    if (is_float) {
        double d = strtod(numstr, NULL);
        result = airl_float(d);
    } else {
        long long ll = strtoll(numstr, NULL, 10);
        result = airl_int((int64_t)ll);
    }
    free(numstr);
    return result;
}

static RtValue *jp_parse_array(JsonParser *p) {
    /* Opening '[' already consumed */
    size_t cap = 8;
    size_t len = 0;
    RtValue **items = malloc(cap * sizeof(RtValue*));

    if (jp_peek(p) == ']') {
        p->pos++;  /* skip ws was done in peek */
        jp_skip_ws(p);
        RtValue *list = airl_list_new(items, 0);
        free(items);
        return list;
    }

    while (1) {
        RtValue *val = jp_parse_value(p);
        if (!val) { /* error */
            for (size_t i = 0; i < len; i++) airl_value_release(items[i]);
            free(items);
            return NULL;
        }
        if (len >= cap) {
            cap *= 2;
            items = realloc(items, cap * sizeof(RtValue*));
        }
        items[len++] = val;

        char c = jp_peek(p);
        if (c == ',') { jp_next(p); continue; }
        if (c == ']') { jp_next(p); break; }
        p->error = "expected ',' or ']'";
        for (size_t i = 0; i < len; i++) airl_value_release(items[i]);
        free(items);
        return NULL;
    }

    RtValue *list = airl_list_new(items, len);
    for (size_t i = 0; i < len; i++) airl_value_release(items[i]);
    free(items);
    return list;
}

static RtValue *jp_parse_object(JsonParser *p) {
    /* Opening '{' already consumed */
    RtValue *map = airl_map_new();

    if (jp_peek(p) == '}') {
        jp_next(p);
        return map;
    }

    while (1) {
        /* Parse key (must be string) */
        if (jp_peek(p) != '"') {
            p->error = "expected string key";
            airl_value_release(map);
            return NULL;
        }
        RtValue *key = jp_parse_string(p);
        if (!key) { airl_value_release(map); return NULL; }

        if (!jp_expect(p, ':')) {
            airl_value_release(key);
            airl_value_release(map);
            return NULL;
        }

        RtValue *val = jp_parse_value(p);
        if (!val) {
            airl_value_release(key);
            airl_value_release(map);
            return NULL;
        }

        RtValue *new_map = airl_map_set(map, key, val);
        airl_value_release(map);
        airl_value_release(key);
        airl_value_release(val);
        map = new_map;

        char c = jp_peek(p);
        if (c == ',') { jp_next(p); continue; }
        if (c == '}') { jp_next(p); break; }
        p->error = "expected ',' or '}'";
        airl_value_release(map);
        return NULL;
    }

    return map;
}

static RtValue *jp_parse_value(JsonParser *p) {
    char c = jp_peek(p);
    if (c == '\0') { p->error = "unexpected end of input"; return NULL; }

    if (c == '"') return jp_parse_string(p);
    if (c == '{') { jp_next(p); return jp_parse_object(p); }
    if (c == '[') { jp_next(p); return jp_parse_array(p); }
    if (c == '-' || isdigit((unsigned char)c)) return jp_parse_number(p);

    /* true / false / null */
    if (p->pos + 4 <= p->len && memcmp(p->src + p->pos, "true", 4) == 0) {
        p->pos += 4;
        return airl_bool(1);
    }
    if (p->pos + 5 <= p->len && memcmp(p->src + p->pos, "false", 5) == 0) {
        p->pos += 5;
        return airl_bool(0);
    }
    if (p->pos + 4 <= p->len && memcmp(p->src + p->pos, "null", 4) == 0) {
        p->pos += 4;
        return airl_nil();
    }

    p->error = "unexpected character";
    return NULL;
}

/* ------------------------------------------------------------------ */
/*  Public: json-parse                                                */
/* ------------------------------------------------------------------ */

RtValue *airl_json_parse(RtValue *text) {
    if (text->tag != RT_STR) {
        return make_err("json-parse: expected string");
    }

    JsonParser p;
    p.src   = text->data.s.ptr;
    p.len   = text->data.s.len;
    p.pos   = 0;
    p.error = NULL;

    RtValue *val = jp_parse_value(&p);
    if (!val) {
        return make_err(p.error ? p.error : "json parse error");
    }

    RtValue *result = make_ok(val);
    return result;
}

/* ------------------------------------------------------------------ */
/*  JSON Stringify                                                    */
/* ------------------------------------------------------------------ */

static void stringify_value(RtValue *v, JsonBuf *buf);

static void stringify_string(const char *s, size_t len, JsonBuf *buf) {
    jbuf_push(buf, '"');
    for (size_t i = 0; i < len; i++) {
        unsigned char c = (unsigned char)s[i];
        switch (c) {
            case '"':  jbuf_append(buf, "\\\"", 2); break;
            case '\\': jbuf_append(buf, "\\\\", 2); break;
            case '\b': jbuf_append(buf, "\\b", 2);  break;
            case '\f': jbuf_append(buf, "\\f", 2);  break;
            case '\n': jbuf_append(buf, "\\n", 2);  break;
            case '\r': jbuf_append(buf, "\\r", 2);  break;
            case '\t': jbuf_append(buf, "\\t", 2);  break;
            default:
                if (c < 0x20) {
                    char esc[7];
                    snprintf(esc, sizeof(esc), "\\u%04x", c);
                    jbuf_append(buf, esc, 6);
                } else {
                    jbuf_push(buf, (char)c);
                }
                break;
        }
    }
    jbuf_push(buf, '"');
}

static void stringify_value(RtValue *v, JsonBuf *buf) {
    if (!v) { jbuf_append_cstr(buf, "null"); return; }

    switch (v->tag) {
        case RT_NIL:
            jbuf_append_cstr(buf, "null");
            break;
        case RT_BOOL:
            jbuf_append_cstr(buf, v->data.b ? "true" : "false");
            break;
        case RT_INT: {
            char num[32];
            int n = snprintf(num, sizeof(num), "%lld", (long long)v->data.i);
            jbuf_append(buf, num, (size_t)n);
            break;
        }
        case RT_FLOAT: {
            char num[64];
            int n = snprintf(num, sizeof(num), "%g", v->data.f);
            jbuf_append(buf, num, (size_t)n);
            break;
        }
        case RT_STR:
            stringify_string(v->data.s.ptr, v->data.s.len, buf);
            break;
        case RT_LIST: {
            jbuf_push(buf, '[');
            for (size_t i = 0; i < v->data.list.len; i++) {
                if (i > 0) jbuf_push(buf, ',');
                stringify_value(v->data.list.items[i], buf);
            }
            jbuf_push(buf, ']');
            break;
        }
        case RT_MAP: {
            jbuf_push(buf, '{');
            int first = 1;
            for (size_t i = 0; i < v->data.map.capacity; i++) {
                MapEntry *e = &v->data.map.entries[i];
                if (!e->occupied || e->deleted) continue;
                if (!first) jbuf_push(buf, ',');
                first = 0;
                stringify_string(e->key, e->key_len, buf);
                jbuf_push(buf, ':');
                stringify_value(e->value, buf);
            }
            jbuf_push(buf, '}');
            break;
        }
        case RT_VARIANT: {
            jbuf_push(buf, '{');
            jbuf_append_cstr(buf, "\"tag\":");
            stringify_string(v->data.variant.tag_name, strlen(v->data.variant.tag_name), buf);
            jbuf_append_cstr(buf, ",\"value\":");
            stringify_value(v->data.variant.inner, buf);
            jbuf_push(buf, '}');
            break;
        }
        default:
            jbuf_append_cstr(buf, "null");
            break;
    }
}

RtValue *airl_json_stringify(RtValue *value) {
    JsonBuf buf;
    jbuf_init(&buf);
    stringify_value(value, &buf);
    RtValue *result = airl_str(buf.data, buf.len);
    free(buf.data);
    return result;
}
