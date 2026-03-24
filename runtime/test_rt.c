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
    printf("\n%d passed, %d failed\n", tests_passed, tests_failed);
    return tests_failed > 0 ? 1 : 0;
}
