#ifndef AIRL_RT_H
#define AIRL_RT_H

#include <stdint.h>
#include <stddef.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* Tags */
#define RT_NIL      0
#define RT_INT      1
#define RT_FLOAT    2
#define RT_BOOL     3
#define RT_STR      4
#define RT_LIST     5
#define RT_MAP      6
#define RT_VARIANT  7
#define RT_CLOSURE  8
#define RT_UNIT     9

/* Forward declare for self-referential types */
typedef struct RtValue RtValue;

/* Map entry for hash table */
typedef struct {
    char* key;
    size_t key_len;
    RtValue* value;
    bool occupied;
    bool deleted;
} MapEntry;

typedef struct RtValue {
    uint8_t tag;
    uint32_t rc;
    union {
        int64_t i;              /* RT_INT */
        double f;               /* RT_FLOAT */
        int64_t b;              /* RT_BOOL (0 or 1) */
        struct {                /* RT_STR */
            char* ptr;
            size_t len;
        } s;
        struct {                /* RT_LIST */
            RtValue** items;
            size_t len;
            size_t cap;
        } list;
        struct {                /* RT_MAP */
            MapEntry* entries;
            size_t capacity;
            size_t count;
        } map;
        struct {                /* RT_VARIANT */
            char* tag_name;
            RtValue* inner;
        } variant;
        struct {                /* RT_CLOSURE */
            void* fn_ptr;
            RtValue** captures;
            size_t cap_count;
        } closure;
    } data;
} RtValue;

/* Memory management */
void airl_value_retain(RtValue* v);
void airl_value_release(RtValue* v);
RtValue* airl_value_clone(RtValue* v);

/* Constructors */
RtValue* airl_int(int64_t v);
RtValue* airl_float(double v);
RtValue* airl_bool(int64_t v);
RtValue* airl_nil(void);
RtValue* airl_unit(void);
RtValue* airl_str(const char* ptr, size_t len);

/* Logic (raw) */
int64_t airl_as_bool_raw(RtValue* v);

/* Arithmetic */
RtValue* airl_add(RtValue* a, RtValue* b);
RtValue* airl_sub(RtValue* a, RtValue* b);
RtValue* airl_mul(RtValue* a, RtValue* b);
RtValue* airl_div(RtValue* a, RtValue* b);
RtValue* airl_mod(RtValue* a, RtValue* b);

/* Comparison */
RtValue* airl_eq(RtValue* a, RtValue* b);
RtValue* airl_ne(RtValue* a, RtValue* b);
RtValue* airl_lt(RtValue* a, RtValue* b);
RtValue* airl_gt(RtValue* a, RtValue* b);
RtValue* airl_le(RtValue* a, RtValue* b);
RtValue* airl_ge(RtValue* a, RtValue* b);

/* Logic */
RtValue* airl_not(RtValue* a);
RtValue* airl_and(RtValue* a, RtValue* b);
RtValue* airl_or(RtValue* a, RtValue* b);
RtValue* airl_xor(RtValue* a, RtValue* b);

/* List operations */
RtValue* airl_head(RtValue* list);
RtValue* airl_tail(RtValue* list);
RtValue* airl_cons(RtValue* elem, RtValue* list);
RtValue* airl_empty(RtValue* list);
RtValue* airl_length(RtValue* v);
RtValue* airl_at(RtValue* list, RtValue* index);
RtValue* airl_append(RtValue* list, RtValue* elem);
RtValue* airl_list_new(RtValue** items, size_t count);

/* String operations */
RtValue* airl_char_at(RtValue* s, RtValue* idx);
RtValue* airl_substring(RtValue* s, RtValue* start, RtValue* end);
RtValue* airl_chars(RtValue* s);
RtValue* airl_split(RtValue* s, RtValue* delim);
RtValue* airl_join(RtValue* list, RtValue* sep);
RtValue* airl_contains(RtValue* s, RtValue* sub);
RtValue* airl_starts_with(RtValue* s, RtValue* prefix);
RtValue* airl_ends_with(RtValue* s, RtValue* suffix);
RtValue* airl_index_of(RtValue* s, RtValue* sub);
RtValue* airl_trim(RtValue* s);
RtValue* airl_to_upper(RtValue* s);
RtValue* airl_to_lower(RtValue* s);
RtValue* airl_replace(RtValue* s, RtValue* old, RtValue* new_str);

/* Map operations */
RtValue* airl_map_new(void);
RtValue* airl_map_from(RtValue* pairs);
RtValue* airl_map_get(RtValue* m, RtValue* key);
RtValue* airl_map_get_or(RtValue* m, RtValue* key, RtValue* default_val);
RtValue* airl_map_set(RtValue* m, RtValue* key, RtValue* value);
RtValue* airl_map_has(RtValue* m, RtValue* key);
RtValue* airl_map_remove(RtValue* m, RtValue* key);
RtValue* airl_map_keys(RtValue* m);
RtValue* airl_map_values(RtValue* m);
RtValue* airl_map_size(RtValue* m);

/* Variant / pattern matching */
RtValue* airl_make_variant(RtValue* tag, RtValue* inner);
RtValue* airl_match_tag(RtValue* val, RtValue* tag);

/* Closures */
RtValue* airl_make_closure(void* fn_ptr, RtValue** captures, size_t count);
RtValue* airl_call_closure(RtValue* closure, RtValue** args, int64_t argc);

/* I/O */
RtValue* airl_print(RtValue* v);
RtValue* airl_print_values(RtValue** args, int64_t count);
RtValue* airl_type_of(RtValue* v);
RtValue* airl_valid(RtValue* v);
RtValue* airl_read_file(RtValue* path);
RtValue* airl_get_args(void);
void airl_set_args(int argc, char** argv);

/* Type conversions */
RtValue* airl_int_to_string(RtValue* n);
RtValue* airl_float_to_string(RtValue* n);
RtValue* airl_string_to_int(RtValue* s);

/* Timing */
RtValue* airl_time_now(void);

/* Environment */
RtValue* airl_getenv(RtValue* name);

/* File I/O (extended) */
RtValue* airl_write_file(RtValue* path, RtValue* content);
RtValue* airl_file_exists(RtValue* path);

/* HTTP */
RtValue* airl_http_request(RtValue* method, RtValue* url, RtValue* body, RtValue* headers);

/* JSON */
RtValue* airl_json_parse(RtValue* text);
RtValue* airl_json_stringify(RtValue* value);

/* Process */
RtValue* airl_shell_exec(RtValue* command, RtValue* args);

/* Contract failure */
int64_t airl_jit_contract_fail(int64_t kind, int64_t fn_idx, int64_t clause_idx);

/* Flush / error */
void airl_flush_stdout(void);
void airl_runtime_error(const char* msg, size_t len);

/* Display helper (used by print) */
void display_value(RtValue* v, FILE* out);

#endif /* AIRL_RT_H */
