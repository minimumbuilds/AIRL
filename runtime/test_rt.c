#include "airl_rt.h"
#include <assert.h>
#include <string.h>
#include <math.h>

static int tests_passed = 0;
static int tests_failed = 0;

#define TEST(name) static void name(void)
#define RUN(name) do { printf("  %s... ", #name); name(); printf("PASS\n"); tests_passed++; } while(0)

/* Capture display_value output to a string */
static char display_buf[4096];
static void display_to_string(RtValue* v) {
    FILE* f = fmemopen(display_buf, sizeof(display_buf), "w");
    display_value(v, f);
    fclose(f);
}

TEST(test_int) {
    RtValue* v = airl_int(42);
    assert(v->tag == RT_INT);
    assert(v->data.i == 42);
    assert(v->rc == 1);
    display_to_string(v);
    assert(strcmp(display_buf, "42") == 0);
    airl_value_release(v);
}

TEST(test_int_negative) {
    RtValue* v = airl_int(-7);
    assert(v->data.i == -7);
    display_to_string(v);
    assert(strcmp(display_buf, "-7") == 0);
    airl_value_release(v);
}

TEST(test_int_zero) {
    RtValue* v = airl_int(0);
    assert(v->data.i == 0);
    display_to_string(v);
    assert(strcmp(display_buf, "0") == 0);
    airl_value_release(v);
}

TEST(test_float_whole) {
    RtValue* v = airl_float(3.0);
    display_to_string(v);
    assert(strcmp(display_buf, "3.0") == 0);
    airl_value_release(v);
}

TEST(test_float_frac) {
    RtValue* v = airl_float(3.14);
    display_to_string(v);
    assert(strcmp(display_buf, "3.14") == 0);
    airl_value_release(v);
}

TEST(test_float_negative) {
    RtValue* v = airl_float(-2.5);
    display_to_string(v);
    assert(strcmp(display_buf, "-2.5") == 0);
    airl_value_release(v);
}

TEST(test_float_zero) {
    RtValue* v = airl_float(0.0);
    display_to_string(v);
    assert(strcmp(display_buf, "0.0") == 0);
    airl_value_release(v);
}

TEST(test_bool) {
    RtValue* t = airl_bool(1);
    RtValue* f = airl_bool(0);
    display_to_string(t);
    assert(strcmp(display_buf, "true") == 0);
    display_to_string(f);
    assert(strcmp(display_buf, "false") == 0);
    airl_value_release(t);
    airl_value_release(f);
}

TEST(test_str) {
    RtValue* v = airl_str("hello", 5);
    assert(v->tag == RT_STR);
    assert(v->data.s.len == 5);
    assert(memcmp(v->data.s.ptr, "hello", 5) == 0);
    display_to_string(v);
    assert(strcmp(display_buf, "\"hello\"") == 0);
    airl_value_release(v);
}

TEST(test_str_empty) {
    RtValue* v = airl_str("", 0);
    assert(v->data.s.len == 0);
    display_to_string(v);
    assert(strcmp(display_buf, "\"\"") == 0);
    airl_value_release(v);
}

TEST(test_str_copies_bytes) {
    char buf[] = "temp";
    RtValue* v = airl_str(buf, 4);
    buf[0] = 'X';  /* modify original */
    assert(v->data.s.ptr[0] == 't');  /* copy is unaffected */
    airl_value_release(v);
}

TEST(test_nil) {
    RtValue* v = airl_nil();
    assert(v->tag == RT_NIL);
    display_to_string(v);
    assert(strcmp(display_buf, "nil") == 0);
    airl_value_release(v);
}

TEST(test_unit) {
    RtValue* v = airl_unit();
    assert(v->tag == RT_UNIT);
    display_to_string(v);
    assert(strcmp(display_buf, "()") == 0);
    airl_value_release(v);
}

TEST(test_retain_release) {
    RtValue* v = airl_int(99);
    assert(v->rc == 1);
    airl_value_retain(v);
    assert(v->rc == 2);
    airl_value_release(v);
    assert(v->rc == 1);
    airl_value_release(v);
    /* v is now freed — don't access it */
}

TEST(test_retain_null) {
    airl_value_retain(NULL);  /* should not crash */
}

TEST(test_release_null) {
    airl_value_release(NULL);  /* should not crash */
}

TEST(test_as_bool_raw) {
    RtValue* v;

    v = airl_bool(1);
    assert(airl_as_bool_raw(v) == 1);
    airl_value_release(v);

    v = airl_bool(0);
    assert(airl_as_bool_raw(v) == 0);
    airl_value_release(v);

    v = airl_nil();
    assert(airl_as_bool_raw(v) == 0);
    airl_value_release(v);

    v = airl_int(42);
    assert(airl_as_bool_raw(v) == 1);
    airl_value_release(v);

    v = airl_int(0);
    assert(airl_as_bool_raw(v) == 0);
    airl_value_release(v);

    v = airl_str("hi", 2);
    assert(airl_as_bool_raw(v) == 1);
    airl_value_release(v);

    v = airl_unit();
    assert(airl_as_bool_raw(v) == 0);
    airl_value_release(v);

    v = airl_float(1.0);
    assert(airl_as_bool_raw(v) == 1);
    airl_value_release(v);

    assert(airl_as_bool_raw(NULL) == 0);
}

TEST(test_clone_int) {
    RtValue* v = airl_int(42);
    RtValue* c = airl_value_clone(v);
    assert(c->tag == RT_INT);
    assert(c->data.i == 42);
    assert(c != v);  /* different allocation */
    assert(c->rc == 1);
    airl_value_release(v);
    airl_value_release(c);
}

TEST(test_clone_str) {
    RtValue* v = airl_str("hello", 5);
    RtValue* c = airl_value_clone(v);
    assert(c->tag == RT_STR);
    assert(c->data.s.len == 5);
    assert(memcmp(c->data.s.ptr, "hello", 5) == 0);
    assert(c->data.s.ptr != v->data.s.ptr);  /* different buffer */
    airl_value_release(v);
    airl_value_release(c);
}

TEST(test_clone_nil) {
    RtValue* v = airl_nil();
    RtValue* c = airl_value_clone(v);
    assert(c->tag == RT_NIL);
    assert(c != v);
    airl_value_release(v);
    airl_value_release(c);
}

TEST(test_clone_null) {
    RtValue* c = airl_value_clone(NULL);
    assert(c == NULL);
}

/* ---- Task 2: Arithmetic, Comparison, Logic ---- */

TEST(test_add_int) {
    RtValue* r = airl_add(airl_int(3), airl_int(4));
    assert(r->data.i == 7);
    airl_value_release(r);
}

TEST(test_add_float) {
    RtValue* r = airl_add(airl_float(1.5), airl_float(2.5));
    assert(r->data.f == 4.0);
    airl_value_release(r);
}

TEST(test_add_str) {
    RtValue* r = airl_add(airl_str("hello", 5), airl_str(" world", 6));
    assert(r->data.s.len == 11);
    assert(memcmp(r->data.s.ptr, "hello world", 11) == 0);
    airl_value_release(r);
}

TEST(test_sub_int) {
    RtValue* r = airl_sub(airl_int(10), airl_int(3));
    assert(r->data.i == 7);
    airl_value_release(r);
}

TEST(test_mul_int) {
    RtValue* r = airl_mul(airl_int(6), airl_int(7));
    assert(r->data.i == 42);
    airl_value_release(r);
}

TEST(test_div_int) {
    RtValue* r = airl_div(airl_int(10), airl_int(3));
    assert(r->data.i == 3);  /* integer division */
    airl_value_release(r);
}

TEST(test_mod_int) {
    RtValue* r = airl_mod(airl_int(10), airl_int(3));
    assert(r->data.i == 1);
    airl_value_release(r);
}

TEST(test_eq_int) {
    assert(airl_as_bool_raw(airl_eq(airl_int(42), airl_int(42))) == 1);
    assert(airl_as_bool_raw(airl_eq(airl_int(42), airl_int(43))) == 0);
}

TEST(test_lt_int) {
    assert(airl_as_bool_raw(airl_lt(airl_int(3), airl_int(7))) == 1);
    assert(airl_as_bool_raw(airl_lt(airl_int(7), airl_int(3))) == 0);
}

TEST(test_eq_str) {
    assert(airl_as_bool_raw(airl_eq(airl_str("abc", 3), airl_str("abc", 3))) == 1);
    assert(airl_as_bool_raw(airl_eq(airl_str("abc", 3), airl_str("def", 3))) == 0);
}

TEST(test_not) {
    assert(airl_as_bool_raw(airl_not(airl_bool(1))) == 0);
    assert(airl_as_bool_raw(airl_not(airl_bool(0))) == 1);
}

TEST(test_and_or) {
    assert(airl_as_bool_raw(airl_and(airl_bool(1), airl_bool(1))) == 1);
    assert(airl_as_bool_raw(airl_and(airl_bool(1), airl_bool(0))) == 0);
    assert(airl_as_bool_raw(airl_or(airl_bool(0), airl_bool(1))) == 1);
    assert(airl_as_bool_raw(airl_or(airl_bool(0), airl_bool(0))) == 0);
}

TEST(test_ne_int) {
    assert(airl_as_bool_raw(airl_ne(airl_int(1), airl_int(2))) == 1);
    assert(airl_as_bool_raw(airl_ne(airl_int(5), airl_int(5))) == 0);
}

TEST(test_gt_int) {
    assert(airl_as_bool_raw(airl_gt(airl_int(7), airl_int(3))) == 1);
    assert(airl_as_bool_raw(airl_gt(airl_int(3), airl_int(7))) == 0);
}

TEST(test_le_int) {
    assert(airl_as_bool_raw(airl_le(airl_int(3), airl_int(7))) == 1);
    assert(airl_as_bool_raw(airl_le(airl_int(7), airl_int(7))) == 1);
    assert(airl_as_bool_raw(airl_le(airl_int(8), airl_int(7))) == 0);
}

TEST(test_ge_int) {
    assert(airl_as_bool_raw(airl_ge(airl_int(7), airl_int(3))) == 1);
    assert(airl_as_bool_raw(airl_ge(airl_int(7), airl_int(7))) == 1);
    assert(airl_as_bool_raw(airl_ge(airl_int(3), airl_int(7))) == 0);
}

TEST(test_xor) {
    assert(airl_as_bool_raw(airl_xor(airl_bool(1), airl_bool(0))) == 1);
    assert(airl_as_bool_raw(airl_xor(airl_bool(0), airl_bool(1))) == 1);
    assert(airl_as_bool_raw(airl_xor(airl_bool(1), airl_bool(1))) == 0);
    assert(airl_as_bool_raw(airl_xor(airl_bool(0), airl_bool(0))) == 0);
}

TEST(test_sub_float) {
    RtValue* r = airl_sub(airl_float(5.5), airl_float(2.0));
    assert(r->data.f == 3.5);
    airl_value_release(r);
}

TEST(test_mul_float) {
    RtValue* r = airl_mul(airl_float(3.0), airl_float(2.5));
    assert(r->data.f == 7.5);
    airl_value_release(r);
}

TEST(test_div_float) {
    RtValue* r = airl_div(airl_float(7.0), airl_float(2.0));
    assert(r->data.f == 3.5);
    airl_value_release(r);
}

TEST(test_mod_float) {
    RtValue* r = airl_mod(airl_float(7.5), airl_float(2.0));
    assert(r->data.f == 1.5);
    airl_value_release(r);
}

TEST(test_eq_bool) {
    assert(airl_as_bool_raw(airl_eq(airl_bool(1), airl_bool(1))) == 1);
    assert(airl_as_bool_raw(airl_eq(airl_bool(0), airl_bool(1))) == 0);
}

TEST(test_eq_nil) {
    assert(airl_as_bool_raw(airl_eq(airl_nil(), airl_nil())) == 1);
}

TEST(test_eq_type_mismatch) {
    assert(airl_as_bool_raw(airl_eq(airl_int(1), airl_str("1", 1))) == 0);
}

TEST(test_lt_float) {
    assert(airl_as_bool_raw(airl_lt(airl_float(1.5), airl_float(2.5))) == 1);
    assert(airl_as_bool_raw(airl_lt(airl_float(3.0), airl_float(2.0))) == 0);
}

TEST(test_lt_str) {
    assert(airl_as_bool_raw(airl_lt(airl_str("abc", 3), airl_str("abd", 3))) == 1);
    assert(airl_as_bool_raw(airl_lt(airl_str("abd", 3), airl_str("abc", 3))) == 0);
    assert(airl_as_bool_raw(airl_lt(airl_str("ab", 2), airl_str("abc", 3))) == 1);
}

/* ---- Task 3: List Operations ---- */

TEST(test_list_new_empty) {
    RtValue* l = airl_list_new(NULL, 0);
    assert(l->tag == RT_LIST);
    assert(l->data.list.len == 0);
    airl_value_release(l);
}

TEST(test_list_new) {
    RtValue* items[] = { airl_int(1), airl_int(2), airl_int(3) };
    RtValue* l = airl_list_new(items, 3);
    assert(l->data.list.len == 3);
    display_to_string(l);
    assert(strcmp(display_buf, "[1 2 3]") == 0);
    airl_value_release(l);
    for (int i = 0; i < 3; i++) airl_value_release(items[i]);
}

TEST(test_head_tail) {
    RtValue* items[] = { airl_int(10), airl_int(20), airl_int(30) };
    RtValue* l = airl_list_new(items, 3);
    RtValue* h = airl_head(l);
    assert(h->data.i == 10);
    RtValue* t = airl_tail(l);
    assert(t->data.list.len == 2);
    airl_value_release(h);
    airl_value_release(t);
    airl_value_release(l);
    for (int i = 0; i < 3; i++) airl_value_release(items[i]);
}

TEST(test_cons) {
    RtValue* items[] = { airl_int(2), airl_int(3) };
    RtValue* l = airl_list_new(items, 2);
    RtValue* one = airl_int(1);
    RtValue* l2 = airl_cons(one, l);
    assert(l2->data.list.len == 3);
    display_to_string(l2);
    assert(strcmp(display_buf, "[1 2 3]") == 0);
    airl_value_release(l2);
    airl_value_release(l);
    airl_value_release(one);
    for (int i = 0; i < 2; i++) airl_value_release(items[i]);
}

TEST(test_empty) {
    RtValue* e = airl_list_new(NULL, 0);
    RtValue* r1 = airl_empty(e);
    assert(airl_as_bool_raw(r1) == 1);
    airl_value_release(r1);
    RtValue* items[] = { airl_int(1) };
    RtValue* ne = airl_list_new(items, 1);
    RtValue* r2 = airl_empty(ne);
    assert(airl_as_bool_raw(r2) == 0);
    airl_value_release(r2);
    airl_value_release(e);
    airl_value_release(ne);
    airl_value_release(items[0]);
}

TEST(test_length) {
    RtValue* items[] = { airl_int(1), airl_int(2), airl_int(3) };
    RtValue* l = airl_list_new(items, 3);
    RtValue* len = airl_length(l);
    assert(len->data.i == 3);
    airl_value_release(len);
    airl_value_release(l);
    for (int i = 0; i < 3; i++) airl_value_release(items[i]);
}

TEST(test_length_str) {
    RtValue* s = airl_str("hello", 5);
    RtValue* len = airl_length(s);
    assert(len->data.i == 5);
    airl_value_release(len);
    airl_value_release(s);
}

TEST(test_at) {
    RtValue* items[] = { airl_int(10), airl_int(20), airl_int(30) };
    RtValue* l = airl_list_new(items, 3);
    RtValue* idx = airl_int(1);
    RtValue* v = airl_at(l, idx);
    assert(v->data.i == 20);
    airl_value_release(v);
    airl_value_release(idx);
    airl_value_release(l);
    for (int i = 0; i < 3; i++) airl_value_release(items[i]);
}

TEST(test_append) {
    RtValue* items[] = { airl_int(1), airl_int(2) };
    RtValue* l = airl_list_new(items, 2);
    RtValue* three = airl_int(3);
    RtValue* l2 = airl_append(l, three);
    display_to_string(l2);
    assert(strcmp(display_buf, "[1 2 3]") == 0);
    airl_value_release(l2);
    airl_value_release(l);
    airl_value_release(three);
    for (int i = 0; i < 2; i++) airl_value_release(items[i]);
}

TEST(test_tail_single) {
    RtValue* items[] = { airl_int(42) };
    RtValue* l = airl_list_new(items, 1);
    RtValue* t = airl_tail(l);
    assert(t->tag == RT_LIST);
    assert(t->data.list.len == 0);
    airl_value_release(t);
    airl_value_release(l);
    airl_value_release(items[0]);
}

TEST(test_cons_empty) {
    RtValue* l = airl_list_new(NULL, 0);
    RtValue* one = airl_int(1);
    RtValue* l2 = airl_cons(one, l);
    assert(l2->data.list.len == 1);
    display_to_string(l2);
    assert(strcmp(display_buf, "[1]") == 0);
    airl_value_release(l2);
    airl_value_release(l);
    airl_value_release(one);
}

/* ---- Task 4: String Operations ---- */

TEST(test_char_at) {
    RtValue* s = airl_str("hello", 5);
    RtValue* r = airl_char_at(s, airl_int(0));
    assert(r->data.s.len == 1 && r->data.s.ptr[0] == 'h');
    airl_value_release(r);
    r = airl_char_at(s, airl_int(4));
    assert(r->data.s.len == 1 && r->data.s.ptr[0] == 'o');
    airl_value_release(r);
    airl_value_release(s);
}

TEST(test_char_at_utf8) {
    /* "cafe\xcc\x81" = "café" as 6 bytes: c(1) a(1) f(1) e(1) combining-accent(2) */
    /* Use precomposed: "caf\xc3\xa9" = 5 bytes, 4 codepoints */
    RtValue* s = airl_str("caf\xc3\xa9", 5);
    RtValue* r = airl_char_at(s, airl_int(3));  /* should be e-acute */
    assert(r->data.s.len == 2);  /* e-acute is 2 bytes in UTF-8 */
    assert((unsigned char)r->data.s.ptr[0] == 0xC3);
    assert((unsigned char)r->data.s.ptr[1] == 0xA9);
    airl_value_release(r);
    airl_value_release(s);
}

TEST(test_substring) {
    RtValue* s = airl_str("hello world", 11);
    RtValue* r = airl_substring(s, airl_int(0), airl_int(5));
    assert(r->data.s.len == 5 && memcmp(r->data.s.ptr, "hello", 5) == 0);
    airl_value_release(r);
    airl_value_release(s);
}

TEST(test_substring_utf8) {
    /* "caf\xc3\xa9s" = 6 bytes, 5 codepoints: c a f e-acute s */
    RtValue* s = airl_str("caf\xc3\xa9s", 6);
    RtValue* r = airl_substring(s, airl_int(2), airl_int(4));  /* "fé" */
    assert(r->data.s.len == 3);  /* f(1) + é(2) = 3 bytes */
    assert(r->data.s.ptr[0] == 'f');
    airl_value_release(r);
    airl_value_release(s);
}

TEST(test_chars) {
    RtValue* s = airl_str("abc", 3);
    RtValue* r = airl_chars(s);
    assert(r->tag == RT_LIST);
    assert(r->data.list.len == 3);
    assert(r->data.list.items[0]->data.s.ptr[0] == 'a');
    assert(r->data.list.items[1]->data.s.ptr[0] == 'b');
    assert(r->data.list.items[2]->data.s.ptr[0] == 'c');
    airl_value_release(r);
    airl_value_release(s);
}

TEST(test_chars_empty) {
    RtValue* s = airl_str("", 0);
    RtValue* r = airl_chars(s);
    assert(r->tag == RT_LIST);
    assert(r->data.list.len == 0);
    airl_value_release(r);
    airl_value_release(s);
}

TEST(test_split) {
    RtValue* s = airl_str("a,b,c", 5);
    RtValue* d = airl_str(",", 1);
    RtValue* r = airl_split(s, d);
    assert(r->data.list.len == 3);
    assert(r->data.list.items[0]->data.s.len == 1 && r->data.list.items[0]->data.s.ptr[0] == 'a');
    assert(r->data.list.items[1]->data.s.len == 1 && r->data.list.items[1]->data.s.ptr[0] == 'b');
    assert(r->data.list.items[2]->data.s.len == 1 && r->data.list.items[2]->data.s.ptr[0] == 'c');
    airl_value_release(r);
    airl_value_release(s);
    airl_value_release(d);
}

TEST(test_split_empty_delim) {
    RtValue* s = airl_str("abc", 3);
    RtValue* d = airl_str("", 0);
    RtValue* r = airl_split(s, d);
    assert(r->data.list.len == 3);
    airl_value_release(r);
    airl_value_release(s);
    airl_value_release(d);
}

TEST(test_join) {
    RtValue* items[] = { airl_str("a", 1), airl_str("b", 1), airl_str("c", 1) };
    RtValue* list = airl_list_new(items, 3);
    RtValue* sep = airl_str("-", 1);
    RtValue* r = airl_join(list, sep);
    assert(r->data.s.len == 5 && memcmp(r->data.s.ptr, "a-b-c", 5) == 0);
    airl_value_release(r);
    airl_value_release(list);
    airl_value_release(sep);
    for (int i = 0; i < 3; i++) airl_value_release(items[i]);
}

TEST(test_join_empty) {
    RtValue* list = airl_list_new(NULL, 0);
    RtValue* sep = airl_str(",", 1);
    RtValue* r = airl_join(list, sep);
    assert(r->data.s.len == 0);
    airl_value_release(r);
    airl_value_release(list);
    airl_value_release(sep);
}

TEST(test_contains) {
    RtValue* s = airl_str("hello world", 11);
    RtValue* sub1 = airl_str("world", 5);
    RtValue* sub2 = airl_str("xyz", 3);
    assert(airl_as_bool_raw(airl_contains(s, sub1)) == 1);
    assert(airl_as_bool_raw(airl_contains(s, sub2)) == 0);
    airl_value_release(s);
    airl_value_release(sub1);
    airl_value_release(sub2);
}

TEST(test_starts_with) {
    RtValue* s = airl_str("hello", 5);
    RtValue* p1 = airl_str("hel", 3);
    RtValue* p2 = airl_str("xyz", 3);
    assert(airl_as_bool_raw(airl_starts_with(s, p1)) == 1);
    assert(airl_as_bool_raw(airl_starts_with(s, p2)) == 0);
    airl_value_release(s);
    airl_value_release(p1);
    airl_value_release(p2);
}

TEST(test_ends_with) {
    RtValue* s = airl_str("hello", 5);
    RtValue* suf1 = airl_str("llo", 3);
    RtValue* suf2 = airl_str("xyz", 3);
    assert(airl_as_bool_raw(airl_ends_with(s, suf1)) == 1);
    assert(airl_as_bool_raw(airl_ends_with(s, suf2)) == 0);
    airl_value_release(s);
    airl_value_release(suf1);
    airl_value_release(suf2);
}

TEST(test_index_of) {
    RtValue* s = airl_str("hello world", 11);
    RtValue* sub1 = airl_str("world", 5);
    RtValue* r = airl_index_of(s, sub1);
    assert(r->data.i == 6);
    airl_value_release(r);
    RtValue* sub2 = airl_str("xyz", 3);
    r = airl_index_of(s, sub2);
    assert(r->data.i == -1);
    airl_value_release(r);
    airl_value_release(s);
    airl_value_release(sub1);
    airl_value_release(sub2);
}

TEST(test_index_of_utf8) {
    /* "cafébar" = c a f é b a r, é is 2 bytes. "bar" starts at char index 4 */
    RtValue* s = airl_str("caf\xc3\xa9""bar", 8);
    RtValue* sub = airl_str("bar", 3);
    RtValue* r = airl_index_of(s, sub);
    assert(r->data.i == 4);  /* character index, not byte index */
    airl_value_release(r);
    airl_value_release(s);
    airl_value_release(sub);
}

TEST(test_trim) {
    RtValue* s = airl_str("  hello  ", 9);
    RtValue* r = airl_trim(s);
    assert(r->data.s.len == 5 && memcmp(r->data.s.ptr, "hello", 5) == 0);
    airl_value_release(r);
    airl_value_release(s);
}

TEST(test_trim_tabs_newlines) {
    RtValue* s = airl_str("\t\nhello\r\n", 9);
    RtValue* r = airl_trim(s);
    assert(r->data.s.len == 5 && memcmp(r->data.s.ptr, "hello", 5) == 0);
    airl_value_release(r);
    airl_value_release(s);
}

TEST(test_trim_empty) {
    RtValue* s = airl_str("   ", 3);
    RtValue* r = airl_trim(s);
    assert(r->data.s.len == 0);
    airl_value_release(r);
    airl_value_release(s);
}

TEST(test_to_upper) {
    RtValue* s = airl_str("hello", 5);
    RtValue* r = airl_to_upper(s);
    assert(memcmp(r->data.s.ptr, "HELLO", 5) == 0);
    airl_value_release(r);
    airl_value_release(s);
}

TEST(test_to_lower) {
    RtValue* s = airl_str("HELLO", 5);
    RtValue* r = airl_to_lower(s);
    assert(memcmp(r->data.s.ptr, "hello", 5) == 0);
    airl_value_release(r);
    airl_value_release(s);
}

TEST(test_replace) {
    RtValue* s = airl_str("hello world", 11);
    RtValue* old_s = airl_str("world", 5);
    RtValue* new_s = airl_str("AIRL", 4);
    RtValue* r = airl_replace(s, old_s, new_s);
    assert(r->data.s.len == 10 && memcmp(r->data.s.ptr, "hello AIRL", 10) == 0);
    airl_value_release(r);
    airl_value_release(s);
    airl_value_release(old_s);
    airl_value_release(new_s);
}

TEST(test_replace_multiple) {
    RtValue* s = airl_str("aXbXc", 5);
    RtValue* old_s = airl_str("X", 1);
    RtValue* new_s = airl_str("--", 2);
    RtValue* r = airl_replace(s, old_s, new_s);
    assert(r->data.s.len == 7 && memcmp(r->data.s.ptr, "a--b--c", 7) == 0);
    airl_value_release(r);
    airl_value_release(s);
    airl_value_release(old_s);
    airl_value_release(new_s);
}

TEST(test_replace_empty_pattern) {
    RtValue* s = airl_str("hello", 5);
    RtValue* old_s = airl_str("", 0);
    RtValue* new_s = airl_str("X", 1);
    RtValue* r = airl_replace(s, old_s, new_s);
    assert(r->data.s.len == 5 && memcmp(r->data.s.ptr, "hello", 5) == 0);
    airl_value_release(r);
    airl_value_release(s);
    airl_value_release(old_s);
    airl_value_release(new_s);
}

int main(void) {
    printf("C Runtime Tests (Task 1):\n");
    RUN(test_int);
    RUN(test_int_negative);
    RUN(test_int_zero);
    RUN(test_float_whole);
    RUN(test_float_frac);
    RUN(test_float_negative);
    RUN(test_float_zero);
    RUN(test_bool);
    RUN(test_str);
    RUN(test_str_empty);
    RUN(test_str_copies_bytes);
    RUN(test_nil);
    RUN(test_unit);
    RUN(test_retain_release);
    RUN(test_retain_null);
    RUN(test_release_null);
    RUN(test_as_bool_raw);
    RUN(test_clone_int);
    RUN(test_clone_str);
    RUN(test_clone_nil);
    RUN(test_clone_null);
    printf("\nC Runtime Tests (Task 2):\n");
    RUN(test_add_int);
    RUN(test_add_float);
    RUN(test_add_str);
    RUN(test_sub_int);
    RUN(test_mul_int);
    RUN(test_div_int);
    RUN(test_mod_int);
    RUN(test_eq_int);
    RUN(test_lt_int);
    RUN(test_eq_str);
    RUN(test_not);
    RUN(test_and_or);
    RUN(test_ne_int);
    RUN(test_gt_int);
    RUN(test_le_int);
    RUN(test_ge_int);
    RUN(test_xor);
    RUN(test_sub_float);
    RUN(test_mul_float);
    RUN(test_div_float);
    RUN(test_mod_float);
    RUN(test_eq_bool);
    RUN(test_eq_nil);
    RUN(test_eq_type_mismatch);
    RUN(test_lt_float);
    RUN(test_lt_str);
    printf("\nC Runtime Tests (Task 3):\n");
    RUN(test_list_new_empty);
    RUN(test_list_new);
    RUN(test_head_tail);
    RUN(test_cons);
    RUN(test_empty);
    RUN(test_length);
    RUN(test_length_str);
    RUN(test_at);
    RUN(test_append);
    RUN(test_tail_single);
    RUN(test_cons_empty);
    printf("\nC Runtime Tests (Task 4):\n");
    RUN(test_char_at);
    RUN(test_char_at_utf8);
    RUN(test_substring);
    RUN(test_substring_utf8);
    RUN(test_chars);
    RUN(test_chars_empty);
    RUN(test_split);
    RUN(test_split_empty_delim);
    RUN(test_join);
    RUN(test_join_empty);
    RUN(test_contains);
    RUN(test_starts_with);
    RUN(test_ends_with);
    RUN(test_index_of);
    RUN(test_index_of_utf8);
    RUN(test_trim);
    RUN(test_trim_tabs_newlines);
    RUN(test_trim_empty);
    RUN(test_to_upper);
    RUN(test_to_lower);
    RUN(test_replace);
    RUN(test_replace_multiple);
    RUN(test_replace_empty_pattern);
    printf("\n%d passed, %d failed\n", tests_passed, tests_failed);
    return tests_failed > 0 ? 1 : 0;
}
