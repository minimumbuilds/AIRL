#include "airl_rt.h"
#include <ctype.h>
#include <errno.h>

/* ---- UTF-8 helpers ---- */

static int utf8_char_byte_len(unsigned char byte) {
    if (byte < 0x80) return 1;
    if ((byte & 0xE0) == 0xC0) return 2;
    if ((byte & 0xF0) == 0xE0) return 3;
    if ((byte & 0xF8) == 0xF0) return 4;
    return 1; /* invalid, treat as 1 */
}

static size_t utf8_char_count(const char* s, size_t len) {
    size_t count = 0;
    size_t i = 0;
    while (i < len) {
        i += utf8_char_byte_len((unsigned char)s[i]);
        count++;
    }
    return count;
}

static size_t utf8_byte_offset(const char* s, size_t len, size_t char_index) {
    size_t i = 0;
    size_t count = 0;
    while (i < len && count < char_index) {
        i += utf8_char_byte_len((unsigned char)s[i]);
        count++;
    }
    return i;
}

/* ---- String builtins ---- */

RtValue* airl_char_at(RtValue* s, RtValue* idx) {
    size_t char_count = utf8_char_count(s->data.s.ptr, s->data.s.len);
    int64_t index = idx->data.i;
    if (index < 0 || (size_t)index >= char_count) {
        fprintf(stderr, "char-at: index %lld out of range (length %zu)\n",
                (long long)index, char_count);
        exit(1);
    }
    size_t byte_off = utf8_byte_offset(s->data.s.ptr, s->data.s.len, (size_t)index);
    int clen = utf8_char_byte_len((unsigned char)s->data.s.ptr[byte_off]);
    return airl_str(s->data.s.ptr + byte_off, (size_t)clen);
}

RtValue* airl_substring(RtValue* s, RtValue* start, RtValue* end) {
    size_t start_off = utf8_byte_offset(s->data.s.ptr, s->data.s.len, (size_t)start->data.i);
    size_t end_off = utf8_byte_offset(s->data.s.ptr, s->data.s.len, (size_t)end->data.i);
    if (end_off < start_off) end_off = start_off;
    return airl_str(s->data.s.ptr + start_off, end_off - start_off);
}

RtValue* airl_chars(RtValue* s) {
    size_t char_count = utf8_char_count(s->data.s.ptr, s->data.s.len);
    if (char_count == 0) {
        return airl_list_new(NULL, 0);
    }
    RtValue** items = malloc(char_count * sizeof(RtValue*));
    size_t i = 0;
    size_t idx = 0;
    while (i < s->data.s.len) {
        int clen = utf8_char_byte_len((unsigned char)s->data.s.ptr[i]);
        items[idx] = airl_str(s->data.s.ptr + i, (size_t)clen);
        i += clen;
        idx++;
    }
    RtValue* result = airl_list_new(items, idx);
    /* airl_list_new retains items, so release our refs */
    for (size_t j = 0; j < idx; j++) {
        airl_value_release(items[j]);
    }
    free(items);
    return result;
}

RtValue* airl_split(RtValue* s, RtValue* delim) {
    /* Empty delimiter: split into individual characters */
    if (delim->data.s.len == 0) {
        return airl_chars(s);
    }

    const char* str = s->data.s.ptr;
    size_t slen = s->data.s.len;
    const char* d = delim->data.s.ptr;
    size_t dlen = delim->data.s.len;

    /* Count splits to know how many items we need */
    size_t count = 1;
    for (size_t i = 0; i + dlen <= slen; i++) {
        if (memcmp(str + i, d, dlen) == 0) {
            count++;
            i += dlen - 1; /* -1 because loop increments */
        }
    }

    RtValue** items = malloc(count * sizeof(RtValue*));
    size_t idx = 0;
    size_t start = 0;
    for (size_t i = 0; i + dlen <= slen; i++) {
        if (memcmp(str + i, d, dlen) == 0) {
            items[idx++] = airl_str(str + start, i - start);
            i += dlen - 1;
            start = i + 1;
        }
    }
    /* Last segment */
    items[idx++] = airl_str(str + start, slen - start);

    RtValue* result = airl_list_new(items, idx);
    for (size_t j = 0; j < idx; j++) {
        airl_value_release(items[j]);
    }
    free(items);
    return result;
}

RtValue* airl_join(RtValue* list, RtValue* sep) {
    size_t n = list->data.list.len;
    if (n == 0) {
        return airl_str("", 0);
    }

    /* Calculate total length */
    size_t total = 0;
    for (size_t i = 0; i < n; i++) {
        total += list->data.list.items[i]->data.s.len;
    }
    total += sep->data.s.len * (n - 1);

    char* buf = malloc(total + 1);
    size_t pos = 0;
    for (size_t i = 0; i < n; i++) {
        if (i > 0 && sep->data.s.len > 0) {
            memcpy(buf + pos, sep->data.s.ptr, sep->data.s.len);
            pos += sep->data.s.len;
        }
        RtValue* item = list->data.list.items[i];
        memcpy(buf + pos, item->data.s.ptr, item->data.s.len);
        pos += item->data.s.len;
    }
    buf[total] = '\0';

    RtValue* result = airl_str(buf, total);
    free(buf);
    return result;
}

RtValue* airl_contains(RtValue* s, RtValue* sub) {
    if (sub->data.s.len == 0) return airl_bool(1);
    if (sub->data.s.len > s->data.s.len) return airl_bool(0);
    return airl_bool(strstr(s->data.s.ptr, sub->data.s.ptr) != NULL);
}

RtValue* airl_starts_with(RtValue* s, RtValue* prefix) {
    if (prefix->data.s.len > s->data.s.len) return airl_bool(0);
    return airl_bool(memcmp(s->data.s.ptr, prefix->data.s.ptr, prefix->data.s.len) == 0);
}

RtValue* airl_ends_with(RtValue* s, RtValue* suffix) {
    if (suffix->data.s.len > s->data.s.len) return airl_bool(0);
    size_t offset = s->data.s.len - suffix->data.s.len;
    return airl_bool(memcmp(s->data.s.ptr + offset, suffix->data.s.ptr, suffix->data.s.len) == 0);
}

RtValue* airl_index_of(RtValue* s, RtValue* sub) {
    if (sub->data.s.len == 0) return airl_int(0);
    if (sub->data.s.len > s->data.s.len) return airl_int(-1);
    char* found = strstr(s->data.s.ptr, sub->data.s.ptr);
    if (!found) return airl_int(-1);
    /* Convert byte offset to character index */
    size_t byte_pos = (size_t)(found - s->data.s.ptr);
    size_t char_idx = utf8_char_count(s->data.s.ptr, byte_pos);
    return airl_int((int64_t)char_idx);
}

RtValue* airl_trim(RtValue* s) {
    const char* start = s->data.s.ptr;
    const char* end = s->data.s.ptr + s->data.s.len;
    while (start < end && (*start == ' ' || *start == '\t' || *start == '\n' || *start == '\r')) {
        start++;
    }
    while (end > start && (*(end-1) == ' ' || *(end-1) == '\t' || *(end-1) == '\n' || *(end-1) == '\r')) {
        end--;
    }
    return airl_str(start, (size_t)(end - start));
}

RtValue* airl_to_upper(RtValue* s) {
    char* buf = malloc(s->data.s.len + 1);
    for (size_t i = 0; i < s->data.s.len; i++) {
        buf[i] = (char)toupper((unsigned char)s->data.s.ptr[i]);
    }
    buf[s->data.s.len] = '\0';
    RtValue* result = airl_str(buf, s->data.s.len);
    free(buf);
    return result;
}

RtValue* airl_to_lower(RtValue* s) {
    char* buf = malloc(s->data.s.len + 1);
    for (size_t i = 0; i < s->data.s.len; i++) {
        buf[i] = (char)tolower((unsigned char)s->data.s.ptr[i]);
    }
    buf[s->data.s.len] = '\0';
    RtValue* result = airl_str(buf, s->data.s.len);
    free(buf);
    return result;
}

RtValue* airl_replace(RtValue* s, RtValue* old, RtValue* new_str) {
    const char* str = s->data.s.ptr;
    size_t slen = s->data.s.len;
    const char* oldp = old->data.s.ptr;
    size_t olen = old->data.s.len;
    const char* newp = new_str->data.s.ptr;
    size_t nlen = new_str->data.s.len;

    /* Empty pattern: return original */
    if (olen == 0) {
        return airl_str(str, slen);
    }

    /* Count occurrences to calculate result size */
    size_t count = 0;
    for (size_t i = 0; i + olen <= slen; i++) {
        if (memcmp(str + i, oldp, olen) == 0) {
            count++;
            i += olen - 1;
        }
    }

    if (count == 0) {
        return airl_str(str, slen);
    }

    size_t result_len = slen + count * nlen - count * olen;
    char* buf = malloc(result_len + 1);
    size_t pos = 0;
    size_t i = 0;
    while (i < slen) {
        if (i + olen <= slen && memcmp(str + i, oldp, olen) == 0) {
            memcpy(buf + pos, newp, nlen);
            pos += nlen;
            i += olen;
        } else {
            buf[pos++] = str[i++];
        }
    }
    buf[result_len] = '\0';

    RtValue* result = airl_str(buf, result_len);
    free(buf);
    return result;
}

/* ---- Type conversions ---- */

RtValue* airl_int_to_string(RtValue* n) {
    char buf[32];
    int len = snprintf(buf, sizeof(buf), "%lld", (long long)n->data.i);
    return airl_str(buf, (size_t)len);
}

RtValue* airl_float_to_string(RtValue* n) {
    char buf[64];
    int len = snprintf(buf, sizeof(buf), "%g", n->data.f);
    return airl_str(buf, (size_t)len);
}

RtValue* airl_string_to_int(RtValue* s) {
    /* Null-terminate the string */
    char* cstr = malloc(s->data.s.len + 1);
    memcpy(cstr, s->data.s.ptr, s->data.s.len);
    cstr[s->data.s.len] = '\0';

    char* endptr;
    errno = 0;
    long long val = strtoll(cstr, &endptr, 10);

    if (errno != 0 || endptr == cstr || *endptr != '\0') {
        free(cstr);
        RtValue* tag = airl_str("Err", 3);
        RtValue* msg = airl_str("invalid integer", 15);
        RtValue* result = airl_make_variant(tag, msg);
        airl_value_release(tag);
        return result;
    }

    free(cstr);
    RtValue* tag = airl_str("Ok", 2);
    RtValue* ival = airl_int((int64_t)val);
    RtValue* result = airl_make_variant(tag, ival);
    airl_value_release(tag);
    return result;
}
