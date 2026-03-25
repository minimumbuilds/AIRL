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

/* ---- COW List Optimization Tests ---- */

TEST(test_tail_cow_elements) {
    /* tail returns correct elements via COW view */
    RtValue* items[] = { airl_int(10), airl_int(20), airl_int(30) };
    RtValue* l = airl_list_new(items, 3);
    RtValue* t = airl_tail(l);
    assert(t->data.list.len == 2);
    display_to_string(t);
    assert(strcmp(display_buf, "[20 30]") == 0);
    /* verify head of tail */
    RtValue* h = airl_head(t);
    assert(h->data.i == 20);
    airl_value_release(h);
    airl_value_release(t);
    airl_value_release(l);
    for (int i = 0; i < 3; i++) airl_value_release(items[i]);
}

TEST(test_tail_of_tail) {
    /* nested tail views */
    RtValue* items[] = { airl_int(1), airl_int(2), airl_int(3), airl_int(4) };
    RtValue* l = airl_list_new(items, 4);
    RtValue* t1 = airl_tail(l);
    RtValue* t2 = airl_tail(t1);
    assert(t2->data.list.len == 2);
    display_to_string(t2);
    assert(strcmp(display_buf, "[3 4]") == 0);
    /* at on nested tail */
    RtValue* idx0 = airl_int(0);
    RtValue* v0 = airl_at(t2, idx0);
    assert(v0->data.i == 3);
    RtValue* idx1 = airl_int(1);
    RtValue* v1 = airl_at(t2, idx1);
    assert(v1->data.i == 4);
    airl_value_release(v0);
    airl_value_release(v1);
    airl_value_release(idx0);
    airl_value_release(idx1);
    airl_value_release(t2);
    airl_value_release(t1);
    airl_value_release(l);
    for (int i = 0; i < 4; i++) airl_value_release(items[i]);
}

TEST(test_tail_parent_freed_after_view) {
    /* release original list before view — view keeps parent alive */
    RtValue* items[] = { airl_int(100), airl_int(200) };
    RtValue* l = airl_list_new(items, 2);
    RtValue* t = airl_tail(l);
    /* release original — tail view should keep data alive via parent ref */
    airl_value_release(l);
    assert(t->data.list.len == 1);
    RtValue* h = airl_head(t);
    assert(h->data.i == 200);
    airl_value_release(h);
    airl_value_release(t);
    for (int i = 0; i < 2; i++) airl_value_release(items[i]);
}

TEST(test_append_inplace_sole_owner) {
    /* when list has rc==1, append should reuse the same pointer */
    RtValue* items[] = { airl_int(1), airl_int(2) };
    RtValue* l = airl_list_new(items, 2);
    /* l has rc=1 but cap==2==len, so it must grow. Verify correctness. */
    RtValue* three = airl_int(3);
    RtValue* l2 = airl_append(l, three);
    /* l2 should be the same pointer as l (in-place realloc) */
    assert(l2 == l);
    assert(l2->data.list.len == 3);
    display_to_string(l2);
    assert(strcmp(display_buf, "[1 2 3]") == 0);
    /* l2 was retained for the caller, release both refs */
    airl_value_release(l2);  /* drops the extra retain from append */
    airl_value_release(l);   /* our original ref — but l==l2, so this is the last ref */
    /* Actually l2==l and both release calls are on the same pointer.
       After the first release rc goes from 2->1, second release frees it. */
    airl_value_release(three);
    for (int i = 0; i < 2; i++) airl_value_release(items[i]);
}

TEST(test_append_copies_when_shared) {
    /* when list has rc>1, append must copy */
    RtValue* items[] = { airl_int(1), airl_int(2) };
    RtValue* l = airl_list_new(items, 2);
    airl_value_retain(l);  /* simulate sharing: rc=2 */
    RtValue* three = airl_int(3);
    RtValue* l2 = airl_append(l, three);
    /* must be a different pointer */
    assert(l2 != l);
    assert(l2->data.list.len == 3);
    /* original list unchanged */
    assert(l->data.list.len == 2);
    display_to_string(l2);
    assert(strcmp(display_buf, "[1 2 3]") == 0);
    airl_value_release(l2);
    airl_value_release(l);  /* drop the extra retain */
    airl_value_release(l);  /* drop original */
    airl_value_release(three);
    for (int i = 0; i < 2; i++) airl_value_release(items[i]);
}

TEST(test_cons_on_tail_view) {
    /* cons on a tail view should produce correct list */
    RtValue* items[] = { airl_int(1), airl_int(2), airl_int(3) };
    RtValue* l = airl_list_new(items, 3);
    RtValue* t = airl_tail(l);  /* [2, 3] */
    RtValue* zero = airl_int(0);
    RtValue* l2 = airl_cons(zero, t);  /* [0, 2, 3] */
    assert(l2->data.list.len == 3);
    display_to_string(l2);
    assert(strcmp(display_buf, "[0 2 3]") == 0);
    airl_value_release(l2);
    airl_value_release(zero);
    airl_value_release(t);
    airl_value_release(l);
    for (int i = 0; i < 3; i++) airl_value_release(items[i]);
}

TEST(test_length_on_tail_view) {
    RtValue* items[] = { airl_int(10), airl_int(20), airl_int(30) };
    RtValue* l = airl_list_new(items, 3);
    RtValue* t = airl_tail(l);
    RtValue* len = airl_length(t);
    assert(len->data.i == 2);
    airl_value_release(len);
    airl_value_release(t);
    airl_value_release(l);
    for (int i = 0; i < 3; i++) airl_value_release(items[i]);
}

TEST(test_empty_on_tail_view) {
    RtValue* items[] = { airl_int(42) };
    RtValue* l = airl_list_new(items, 1);
    RtValue* t = airl_tail(l);
    RtValue* e = airl_empty(t);
    assert(airl_as_bool_raw(e) == 1);
    airl_value_release(e);
    airl_value_release(t);
    airl_value_release(l);
    airl_value_release(items[0]);
}

TEST(test_fold_pattern_with_tail) {
    /* Simulate a fold: sum a list by repeatedly calling head/tail.
       This is the hot path that was O(N^2) before COW. */
    RtValue* items[100];
    for (int i = 0; i < 100; i++) items[i] = airl_int(i + 1);
    RtValue* l = airl_list_new(items, 100);
    int64_t sum = 0;
    RtValue* cur = l;
    airl_value_retain(cur);
    while (1) {
        RtValue* emp = airl_empty(cur);
        int is_empty = airl_as_bool_raw(emp);
        airl_value_release(emp);
        if (is_empty) break;
        RtValue* h = airl_head(cur);
        sum += h->data.i;
        airl_value_release(h);
        RtValue* next = airl_tail(cur);
        airl_value_release(cur);
        cur = next;
    }
    airl_value_release(cur);
    assert(sum == 5050);  /* 1+2+...+100 = 5050 */
    airl_value_release(l);
    for (int i = 0; i < 100; i++) airl_value_release(items[i]);
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

/* ---- Task 5: Map Operations ---- */

TEST(test_map_new) {
    RtValue* m = airl_map_new();
    assert(m->tag == RT_MAP);
    assert(m->data.map.count == 0);
    display_to_string(m);
    assert(strcmp(display_buf, "{}") == 0);
    airl_value_release(m);
}

TEST(test_map_set_get) {
    RtValue* m = airl_map_new();
    RtValue* m2 = airl_map_set(m, airl_str("name", 4), airl_str("AIRL", 4));
    RtValue* v = airl_map_get(m2, airl_str("name", 4));
    assert(v->tag == RT_STR);
    assert(memcmp(v->data.s.ptr, "AIRL", 4) == 0);
    airl_value_release(v);
    airl_value_release(m2);
    airl_value_release(m);
}

TEST(test_map_get_missing) {
    RtValue* m = airl_map_new();
    RtValue* v = airl_map_get(m, airl_str("nope", 4));
    assert(v->tag == RT_NIL);
    airl_value_release(v);
    airl_value_release(m);
}

TEST(test_map_has) {
    RtValue* m = airl_map_set(airl_map_new(), airl_str("x", 1), airl_int(1));
    assert(airl_as_bool_raw(airl_map_has(m, airl_str("x", 1))) == 1);
    assert(airl_as_bool_raw(airl_map_has(m, airl_str("y", 1))) == 0);
    airl_value_release(m);
}

TEST(test_map_size) {
    RtValue* m = airl_map_set(airl_map_set(airl_map_new(), airl_str("a", 1), airl_int(1)), airl_str("b", 1), airl_int(2));
    RtValue* s = airl_map_size(m);
    assert(s->data.i == 2);
    airl_value_release(s);
    airl_value_release(m);
}

TEST(test_map_keys_sorted) {
    RtValue* m = airl_map_set(airl_map_set(airl_map_set(airl_map_new(),
        airl_str("c", 1), airl_int(3)),
        airl_str("a", 1), airl_int(1)),
        airl_str("b", 1), airl_int(2));
    RtValue* keys = airl_map_keys(m);
    assert(keys->data.list.len == 3);
    /* Must be sorted: a, b, c */
    RtValue* k0 = keys->data.list.items[0];
    RtValue* k1 = keys->data.list.items[1];
    RtValue* k2 = keys->data.list.items[2];
    assert(memcmp(k0->data.s.ptr, "a", 1) == 0);
    assert(memcmp(k1->data.s.ptr, "b", 1) == 0);
    assert(memcmp(k2->data.s.ptr, "c", 1) == 0);
    airl_value_release(keys);
    airl_value_release(m);
}

TEST(test_map_from) {
    RtValue* items[] = {
        airl_str("x", 1), airl_int(10),
        airl_str("y", 1), airl_int(20)
    };
    RtValue* pairs = airl_list_new(items, 4);
    RtValue* m = airl_map_from(pairs);
    assert(m->data.map.count == 2);
    RtValue* v = airl_map_get(m, airl_str("x", 1));
    assert(v->data.i == 10);
    airl_value_release(v);
    airl_value_release(m);
    airl_value_release(pairs);
    for (int i = 0; i < 4; i++) airl_value_release(items[i]);
}

TEST(test_map_remove) {
    RtValue* m = airl_map_set(airl_map_set(airl_map_new(),
        airl_str("a", 1), airl_int(1)),
        airl_str("b", 1), airl_int(2));
    RtValue* m2 = airl_map_remove(m, airl_str("a", 1));
    assert(m2->data.map.count == 1);
    assert(airl_as_bool_raw(airl_map_has(m2, airl_str("a", 1))) == 0);
    assert(airl_as_bool_raw(airl_map_has(m2, airl_str("b", 1))) == 1);
    airl_value_release(m2);
    airl_value_release(m);
}

TEST(test_map_display) {
    RtValue* m = airl_map_set(airl_map_set(airl_map_new(),
        airl_str("b", 1), airl_int(2)),
        airl_str("a", 1), airl_int(1));
    display_to_string(m);
    /* Must be sorted by key: {a: 1 b: 2} */
    assert(strcmp(display_buf, "{a: 1 b: 2}") == 0);
    airl_value_release(m);
}

TEST(test_map_get_or) {
    RtValue* m = airl_map_set(airl_map_new(), airl_str("x", 1), airl_int(42));
    RtValue* v1 = airl_map_get_or(m, airl_str("x", 1), airl_int(0));
    assert(v1->data.i == 42);
    airl_value_release(v1);
    RtValue* def = airl_int(99);
    RtValue* v2 = airl_map_get_or(m, airl_str("missing", 7), def);
    assert(v2->data.i == 99);
    airl_value_release(v2);
    airl_value_release(def);
    airl_value_release(m);
}

TEST(test_map_values_sorted) {
    RtValue* m = airl_map_set(airl_map_set(airl_map_set(airl_map_new(),
        airl_str("c", 1), airl_int(30)),
        airl_str("a", 1), airl_int(10)),
        airl_str("b", 1), airl_int(20));
    RtValue* vals = airl_map_values(m);
    assert(vals->data.list.len == 3);
    /* Values in key-sorted order: a->10, b->20, c->30 */
    assert(vals->data.list.items[0]->data.i == 10);
    assert(vals->data.list.items[1]->data.i == 20);
    assert(vals->data.list.items[2]->data.i == 30);
    airl_value_release(vals);
    airl_value_release(m);
}

TEST(test_map_overwrite) {
    RtValue* m = airl_map_set(airl_map_new(), airl_str("x", 1), airl_int(1));
    RtValue* m2 = airl_map_set(m, airl_str("x", 1), airl_int(99));
    assert(m2->data.map.count == 1);
    RtValue* v = airl_map_get(m2, airl_str("x", 1));
    assert(v->data.i == 99);
    /* Original unchanged */
    RtValue* v_orig = airl_map_get(m, airl_str("x", 1));
    assert(v_orig->data.i == 1);
    airl_value_release(v);
    airl_value_release(v_orig);
    airl_value_release(m2);
    airl_value_release(m);
}

/* ---- Task 6: Variants, Closures, I/O ---- */

TEST(test_make_variant) {
    RtValue* tag = airl_str("Ok", 2);
    RtValue* inner = airl_int(42);
    RtValue* v = airl_make_variant(tag, inner);
    assert(v->tag == RT_VARIANT);
    display_to_string(v);
    assert(strcmp(display_buf, "(Ok 42)") == 0);
    airl_value_release(v);
    airl_value_release(inner);
    airl_value_release(tag);
}

TEST(test_make_variant_unit_inner) {
    RtValue* tag = airl_str("None", 4);
    RtValue* inner = airl_unit();
    RtValue* v = airl_make_variant(tag, inner);
    assert(v->tag == RT_VARIANT);
    display_to_string(v);
    assert(strcmp(display_buf, "(None)") == 0);
    airl_value_release(v);
    airl_value_release(inner);
    airl_value_release(tag);
}

TEST(test_match_tag_success) {
    RtValue* tag = airl_str("Ok", 2);
    RtValue* inner = airl_int(42);
    RtValue* v = airl_make_variant(tag, inner);
    RtValue* match_tag = airl_str("Ok", 2);
    RtValue* result = airl_match_tag(v, match_tag);
    assert(result != NULL);
    assert(result->data.i == 42);
    airl_value_release(result);
    airl_value_release(v);
    airl_value_release(inner);
    airl_value_release(tag);
    airl_value_release(match_tag);
}

TEST(test_match_tag_fail) {
    RtValue* tag = airl_str("Ok", 2);
    RtValue* inner = airl_int(42);
    RtValue* v = airl_make_variant(tag, inner);
    RtValue* match_tag = airl_str("Err", 3);
    RtValue* result = airl_match_tag(v, match_tag);
    assert(result == NULL);
    airl_value_release(v);
    airl_value_release(inner);
    airl_value_release(tag);
    airl_value_release(match_tag);
}

TEST(test_match_tag_not_variant) {
    RtValue* v = airl_int(42);
    RtValue* tag = airl_str("Ok", 2);
    RtValue* result = airl_match_tag(v, tag);
    assert(result == NULL);
    airl_value_release(v);
    airl_value_release(tag);
}

TEST(test_nested_variant) {
    RtValue* tag_inner = airl_str("Ok", 2);
    RtValue* inner = airl_make_variant(tag_inner, airl_int(7));
    RtValue* tag_outer = airl_str("Some", 4);
    RtValue* v = airl_make_variant(tag_outer, inner);
    display_to_string(v);
    assert(strcmp(display_buf, "(Some (Ok 7))") == 0);
    airl_value_release(v);
    airl_value_release(inner);
    airl_value_release(tag_inner);
    airl_value_release(tag_outer);
}

/* Closure test helpers */
static RtValue* test_closure_double(RtValue* x) {
    return airl_mul(x, airl_int(2));
}

static RtValue* test_closure_add(RtValue* a, RtValue* b) {
    return airl_add(a, b);
}

TEST(test_closure_no_capture) {
    RtValue* c = airl_make_closure((void*)test_closure_double, NULL, 0);
    assert(c->tag == RT_CLOSURE);
    RtValue* arg = airl_int(21);
    RtValue* result = airl_call_closure(c, &arg, 1);
    assert(result->data.i == 42);
    airl_value_release(result);
    airl_value_release(c);
    airl_value_release(arg);
}

TEST(test_closure_with_capture) {
    RtValue* offset = airl_int(10);
    RtValue* c = airl_make_closure((void*)test_closure_add, &offset, 1);
    RtValue* arg = airl_int(5);
    RtValue* result = airl_call_closure(c, &arg, 1);
    assert(result->data.i == 15);
    airl_value_release(result);
    airl_value_release(c);
    airl_value_release(offset);
    airl_value_release(arg);
}

TEST(test_closure_display) {
    RtValue* c = airl_make_closure((void*)test_closure_double, NULL, 0);
    display_to_string(c);
    assert(strcmp(display_buf, "<closure>") == 0);
    airl_value_release(c);
}

TEST(test_type_of_int) {
    RtValue* t = airl_type_of(airl_int(1));
    assert(t->data.s.len == 3 && memcmp(t->data.s.ptr, "Int", 3) == 0);
    airl_value_release(t);
}

TEST(test_type_of_str) {
    RtValue* s = airl_str("hi", 2);
    RtValue* t = airl_type_of(s);
    assert(t->data.s.len == 3 && memcmp(t->data.s.ptr, "Str", 3) == 0);
    airl_value_release(t);
    airl_value_release(s);
}

TEST(test_type_of_float) {
    RtValue* t = airl_type_of(airl_float(3.14));
    assert(t->data.s.len == 5 && memcmp(t->data.s.ptr, "Float", 5) == 0);
    airl_value_release(t);
}

TEST(test_type_of_bool) {
    RtValue* t = airl_type_of(airl_bool(1));
    assert(t->data.s.len == 4 && memcmp(t->data.s.ptr, "Bool", 4) == 0);
    airl_value_release(t);
}

TEST(test_type_of_nil) {
    RtValue* t = airl_type_of(airl_nil());
    assert(t->data.s.len == 3 && memcmp(t->data.s.ptr, "Nil", 3) == 0);
    airl_value_release(t);
}

TEST(test_type_of_list) {
    RtValue* l = airl_list_new(NULL, 0);
    RtValue* t = airl_type_of(l);
    assert(t->data.s.len == 4 && memcmp(t->data.s.ptr, "List", 4) == 0);
    airl_value_release(t);
    airl_value_release(l);
}

TEST(test_type_of_variant) {
    RtValue* tag = airl_str("Ok", 2);
    RtValue* v = airl_make_variant(tag, airl_int(1));
    RtValue* t = airl_type_of(v);
    assert(t->data.s.len == 7 && memcmp(t->data.s.ptr, "Variant", 7) == 0);
    airl_value_release(t);
    airl_value_release(v);
    airl_value_release(tag);
}

TEST(test_type_of_closure) {
    RtValue* c = airl_make_closure((void*)test_closure_double, NULL, 0);
    RtValue* t = airl_type_of(c);
    assert(t->data.s.len == 7 && memcmp(t->data.s.ptr, "Closure", 7) == 0);
    airl_value_release(t);
    airl_value_release(c);
}

TEST(test_valid) {
    assert(airl_as_bool_raw(airl_valid(airl_int(1))) == 1);
    assert(airl_as_bool_raw(airl_valid(airl_nil())) == 1);
    assert(airl_as_bool_raw(airl_valid(airl_bool(0))) == 1);
}

TEST(test_get_args) {
    char* fake_argv[] = { "prog", "arg1", "arg2" };
    airl_set_args(3, fake_argv);
    RtValue* args = airl_get_args();
    assert(args->tag == RT_LIST);
    assert(args->data.list.len == 3);
    assert(args->data.list.items[0]->data.s.len == 4 && memcmp(args->data.list.items[0]->data.s.ptr, "prog", 4) == 0);
    assert(args->data.list.items[1]->data.s.len == 4 && memcmp(args->data.list.items[1]->data.s.ptr, "arg1", 4) == 0);
    assert(args->data.list.items[2]->data.s.len == 4 && memcmp(args->data.list.items[2]->data.s.ptr, "arg2", 4) == 0);
    airl_value_release(args);
}

TEST(test_get_args_empty) {
    char* fake_argv[] = { NULL }; /* not used */
    airl_set_args(0, fake_argv);
    RtValue* args = airl_get_args();
    assert(args->tag == RT_LIST);
    assert(args->data.list.len == 0);
    airl_value_release(args);
}

TEST(test_read_file) {
    /* Write a temp file, read it back */
    FILE* f = fopen("/tmp/airl_test_read.txt", "w");
    fprintf(f, "hello from file");
    fclose(f);
    RtValue* path = airl_str("/tmp/airl_test_read.txt", 23);
    RtValue* content = airl_read_file(path);
    assert(content->tag == RT_STR);
    assert(content->data.s.len == 15);
    assert(memcmp(content->data.s.ptr, "hello from file", 15) == 0);
    airl_value_release(content);
    airl_value_release(path);
    remove("/tmp/airl_test_read.txt");
}

TEST(test_contract_fail) {
    int64_t result = airl_jit_contract_fail(1, 2, 3);
    assert(result == 0);
}

/* ---- Task 7: New builtins ---- */

TEST(test_int_to_string) {
    RtValue* n = airl_int(42);
    RtValue* s = airl_int_to_string(n);
    assert(s->tag == RT_STR);
    assert(s->data.s.len == 2);
    assert(memcmp(s->data.s.ptr, "42", 2) == 0);
    airl_value_release(s);
    airl_value_release(n);
}

TEST(test_int_to_string_negative) {
    RtValue* n = airl_int(-7);
    RtValue* s = airl_int_to_string(n);
    assert(s->tag == RT_STR);
    assert(memcmp(s->data.s.ptr, "-7", 2) == 0);
    airl_value_release(s);
    airl_value_release(n);
}

TEST(test_float_to_string) {
    RtValue* n = airl_float(3.14);
    RtValue* s = airl_float_to_string(n);
    assert(s->tag == RT_STR);
    assert(s->data.s.len > 0);
    airl_value_release(s);
    airl_value_release(n);
}

TEST(test_string_to_int_ok) {
    RtValue* s = airl_str("42", 2);
    RtValue* r = airl_string_to_int(s);
    assert(r->tag == RT_VARIANT);
    assert(strcmp(r->data.variant.tag_name, "Ok") == 0);
    assert(r->data.variant.inner->tag == RT_INT);
    assert(r->data.variant.inner->data.i == 42);
    airl_value_release(r);
    airl_value_release(s);
}

TEST(test_string_to_int_err) {
    RtValue* s = airl_str("abc", 3);
    RtValue* r = airl_string_to_int(s);
    assert(r->tag == RT_VARIANT);
    assert(strcmp(r->data.variant.tag_name, "Err") == 0);
    airl_value_release(r);
    airl_value_release(s);
}

TEST(test_time_now) {
    RtValue* t = airl_time_now();
    assert(t->tag == RT_INT);
    assert(t->data.i > 0);
    airl_value_release(t);
}

TEST(test_getenv_path) {
    RtValue* name = airl_str("PATH", 4);
    RtValue* r = airl_getenv(name);
    assert(r->tag == RT_VARIANT);
    assert(strcmp(r->data.variant.tag_name, "Ok") == 0);
    assert(r->data.variant.inner->tag == RT_STR);
    airl_value_release(r);
    airl_value_release(name);
}

TEST(test_getenv_missing) {
    RtValue* name = airl_str("NONEXISTENT_VAR_XYZ_123", 23);
    RtValue* r = airl_getenv(name);
    assert(r->tag == RT_VARIANT);
    assert(strcmp(r->data.variant.tag_name, "Err") == 0);
    airl_value_release(r);
    airl_value_release(name);
}

TEST(test_write_file_and_read) {
    RtValue* path = airl_str("/tmp/airl_test_write.txt", 23);
    RtValue* content = airl_str("test content", 12);
    RtValue* ok = airl_write_file(path, content);
    assert(ok->tag == RT_BOOL);
    assert(ok->data.b == 1);
    /* Read back */
    RtValue* read = airl_read_file(path);
    assert(read->tag == RT_STR);
    assert(read->data.s.len == 12);
    assert(memcmp(read->data.s.ptr, "test content", 12) == 0);
    airl_value_release(read);
    airl_value_release(ok);
    airl_value_release(content);
    airl_value_release(path);
    remove("/tmp/airl_test_write.txt");
}

TEST(test_file_exists) {
    /* Write a temp file first */
    FILE* f = fopen("/tmp/airl_test_exists.txt", "w");
    fprintf(f, "x");
    fclose(f);
    RtValue* path = airl_str("/tmp/airl_test_exists.txt", 25);
    RtValue* r = airl_file_exists(path);
    assert(r->tag == RT_BOOL);
    assert(r->data.b == 1);
    airl_value_release(r);
    airl_value_release(path);
    remove("/tmp/airl_test_exists.txt");
    /* Now check it doesn't exist */
    RtValue* path2 = airl_str("/tmp/airl_test_exists.txt", 25);
    RtValue* r2 = airl_file_exists(path2);
    assert(r2->tag == RT_BOOL);
    assert(r2->data.b == 0);
    airl_value_release(r2);
    airl_value_release(path2);
}

TEST(test_json_parse_object) {
    RtValue* text = airl_str("{\"key\":\"val\",\"num\":42}", 22);
    RtValue* r = airl_json_parse(text);
    assert(r->tag == RT_VARIANT);
    assert(strcmp(r->data.variant.tag_name, "Ok") == 0);
    RtValue* val = r->data.variant.inner;
    assert(val->tag == RT_MAP);
    airl_value_release(r);
    airl_value_release(text);
}

TEST(test_json_parse_array) {
    RtValue* text = airl_str("[1,2,3]", 7);
    RtValue* r = airl_json_parse(text);
    assert(r->tag == RT_VARIANT);
    assert(strcmp(r->data.variant.tag_name, "Ok") == 0);
    RtValue* val = r->data.variant.inner;
    assert(val->tag == RT_LIST);
    assert(val->data.list.len == 3);
    airl_value_release(r);
    airl_value_release(text);
}

TEST(test_json_stringify_roundtrip) {
    RtValue* text = airl_str("{\"a\":1}", 7);
    RtValue* parsed = airl_json_parse(text);
    assert(strcmp(parsed->data.variant.tag_name, "Ok") == 0);
    RtValue* stringified = airl_json_stringify(parsed->data.variant.inner);
    assert(stringified->tag == RT_STR);
    /* Re-parse to verify structure preserved */
    RtValue* reparsed = airl_json_parse(stringified);
    assert(strcmp(reparsed->data.variant.tag_name, "Ok") == 0);
    airl_value_release(reparsed);
    airl_value_release(stringified);
    airl_value_release(parsed);
    airl_value_release(text);
}

TEST(test_shell_exec_echo) {
    RtValue* cmd = airl_str("echo", 4);
    RtValue** items = malloc(sizeof(RtValue*));
    items[0] = airl_str("hello", 5);
    RtValue* args = airl_list_new(items, 1);
    airl_value_release(items[0]);
    free(items);
    RtValue* r = airl_shell_exec(cmd, args);
    assert(r->tag == RT_VARIANT);
    assert(strcmp(r->data.variant.tag_name, "Ok") == 0);
    /* Check stdout contains "hello" */
    RtValue* result_map = r->data.variant.inner;
    assert(result_map->tag == RT_MAP);
    RtValue* stdout_key = airl_str("stdout", 6);
    RtValue* stdout_val = airl_map_get(result_map, stdout_key);
    assert(stdout_val->tag == RT_STR);
    assert(stdout_val->data.s.len >= 5);
    assert(memcmp(stdout_val->data.s.ptr, "hello", 5) == 0);
    airl_value_release(stdout_val);
    airl_value_release(stdout_key);
    airl_value_release(r);
    airl_value_release(args);
    airl_value_release(cmd);
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
    printf("\nC Runtime Tests (COW List Optimizations):\n");
    RUN(test_tail_cow_elements);
    RUN(test_tail_of_tail);
    RUN(test_tail_parent_freed_after_view);
    RUN(test_append_inplace_sole_owner);
    RUN(test_append_copies_when_shared);
    RUN(test_cons_on_tail_view);
    RUN(test_length_on_tail_view);
    RUN(test_empty_on_tail_view);
    RUN(test_fold_pattern_with_tail);
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
    printf("\nC Runtime Tests (Task 5):\n");
    RUN(test_map_new);
    RUN(test_map_set_get);
    RUN(test_map_get_missing);
    RUN(test_map_has);
    RUN(test_map_size);
    RUN(test_map_keys_sorted);
    RUN(test_map_from);
    RUN(test_map_remove);
    RUN(test_map_display);
    RUN(test_map_get_or);
    RUN(test_map_values_sorted);
    RUN(test_map_overwrite);
    printf("\nC Runtime Tests (Task 6):\n");
    RUN(test_make_variant);
    RUN(test_make_variant_unit_inner);
    RUN(test_match_tag_success);
    RUN(test_match_tag_fail);
    RUN(test_match_tag_not_variant);
    RUN(test_nested_variant);
    RUN(test_closure_no_capture);
    RUN(test_closure_with_capture);
    RUN(test_closure_display);
    RUN(test_type_of_int);
    RUN(test_type_of_str);
    RUN(test_type_of_float);
    RUN(test_type_of_bool);
    RUN(test_type_of_nil);
    RUN(test_type_of_list);
    RUN(test_type_of_variant);
    RUN(test_type_of_closure);
    RUN(test_valid);
    RUN(test_get_args);
    RUN(test_get_args_empty);
    RUN(test_read_file);
    RUN(test_contract_fail);
    printf("\nC Runtime Tests (Task 7 - New builtins):\n");
    RUN(test_int_to_string);
    RUN(test_int_to_string_negative);
    RUN(test_float_to_string);
    RUN(test_string_to_int_ok);
    RUN(test_string_to_int_err);
    RUN(test_time_now);
    RUN(test_getenv_path);
    RUN(test_getenv_missing);
    RUN(test_write_file_and_read);
    RUN(test_file_exists);
    RUN(test_json_parse_object);
    RUN(test_json_parse_array);
    RUN(test_json_stringify_roundtrip);
    RUN(test_shell_exec_echo);
    printf("\n%d passed, %d failed\n", tests_passed, tests_failed);
    return tests_failed > 0 ? 1 : 0;
}
