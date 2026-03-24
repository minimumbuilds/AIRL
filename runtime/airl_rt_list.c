#include "airl_rt.h"

/* Internal helper: allocate a list RtValue from an array of items */
static RtValue* make_list(RtValue** items, size_t len) {
    RtValue* v = (RtValue*)malloc(sizeof(RtValue));
    if (!v) {
        fprintf(stderr, "airl_rt: out of memory\n");
        exit(1);
    }
    v->tag = RT_LIST;
    v->rc = 1;
    memset(&v->data, 0, sizeof(v->data));
    v->data.list.len = len;
    v->data.list.cap = len;
    if (len > 0) {
        v->data.list.items = (RtValue**)malloc(len * sizeof(RtValue*));
        if (!v->data.list.items) {
            fprintf(stderr, "airl_rt: out of memory\n");
            exit(1);
        }
        for (size_t i = 0; i < len; i++) {
            v->data.list.items[i] = items[i];
            airl_value_retain(items[i]);
        }
    } else {
        v->data.list.items = NULL;
    }
    return v;
}

RtValue* airl_list_new(RtValue** items, size_t count) {
    if (count == 0 || items == NULL) {
        return make_list(NULL, 0);
    }
    return make_list(items, count);
}

RtValue* airl_head(RtValue* list) {
    if (!list || list->tag != RT_LIST || list->data.list.len == 0) {
        fprintf(stderr, "airl_head: empty list\n");
        exit(1);
    }
    RtValue* h = list->data.list.items[0];
    airl_value_retain(h);
    return h;
}

RtValue* airl_tail(RtValue* list) {
    if (!list || list->tag != RT_LIST || list->data.list.len == 0) {
        fprintf(stderr, "airl_tail: empty list\n");
        exit(1);
    }
    size_t new_len = list->data.list.len - 1;
    return make_list(list->data.list.items + 1, new_len);
}

RtValue* airl_cons(RtValue* elem, RtValue* list) {
    if (!list || list->tag != RT_LIST) {
        fprintf(stderr, "airl_cons: not a list\n");
        exit(1);
    }
    size_t old_len = list->data.list.len;
    size_t new_len = old_len + 1;

    RtValue* v = (RtValue*)malloc(sizeof(RtValue));
    if (!v) {
        fprintf(stderr, "airl_rt: out of memory\n");
        exit(1);
    }
    v->tag = RT_LIST;
    v->rc = 1;
    memset(&v->data, 0, sizeof(v->data));
    v->data.list.len = new_len;
    v->data.list.cap = new_len;
    v->data.list.items = (RtValue**)malloc(new_len * sizeof(RtValue*));
    if (!v->data.list.items) {
        fprintf(stderr, "airl_rt: out of memory\n");
        exit(1);
    }

    v->data.list.items[0] = elem;
    airl_value_retain(elem);
    for (size_t i = 0; i < old_len; i++) {
        v->data.list.items[i + 1] = list->data.list.items[i];
        airl_value_retain(list->data.list.items[i]);
    }
    return v;
}

RtValue* airl_empty(RtValue* list) {
    if (!list || list->tag != RT_LIST) {
        fprintf(stderr, "airl_empty: not a list\n");
        exit(1);
    }
    return airl_bool(list->data.list.len == 0);
}

RtValue* airl_length(RtValue* v) {
    if (!v) {
        fprintf(stderr, "airl_length: null value\n");
        exit(1);
    }
    switch (v->tag) {
        case RT_LIST:
            return airl_int((int64_t)v->data.list.len);
        case RT_STR:
            return airl_int((int64_t)v->data.s.len);
        case RT_MAP:
            return airl_int((int64_t)v->data.map.count);
        default:
            fprintf(stderr, "airl_length: unsupported type (tag=%d)\n", v->tag);
            exit(1);
    }
}

RtValue* airl_at(RtValue* list, RtValue* index) {
    if (!list || list->tag != RT_LIST) {
        fprintf(stderr, "airl_at: not a list\n");
        exit(1);
    }
    if (!index || index->tag != RT_INT) {
        fprintf(stderr, "airl_at: index must be an integer\n");
        exit(1);
    }
    int64_t idx = index->data.i;
    if (idx < 0 || (size_t)idx >= list->data.list.len) {
        fprintf(stderr, "airl_at: index %lld out of range (len=%zu)\n",
                (long long)idx, list->data.list.len);
        exit(1);
    }
    RtValue* item = list->data.list.items[idx];
    airl_value_retain(item);
    return item;
}

RtValue* airl_append(RtValue* list, RtValue* elem) {
    if (!list || list->tag != RT_LIST) {
        fprintf(stderr, "airl_append: not a list\n");
        exit(1);
    }
    size_t old_len = list->data.list.len;
    size_t new_len = old_len + 1;

    RtValue* v = (RtValue*)malloc(sizeof(RtValue));
    if (!v) {
        fprintf(stderr, "airl_rt: out of memory\n");
        exit(1);
    }
    v->tag = RT_LIST;
    v->rc = 1;
    memset(&v->data, 0, sizeof(v->data));
    v->data.list.len = new_len;
    v->data.list.cap = new_len;
    v->data.list.items = (RtValue**)malloc(new_len * sizeof(RtValue*));
    if (!v->data.list.items) {
        fprintf(stderr, "airl_rt: out of memory\n");
        exit(1);
    }

    for (size_t i = 0; i < old_len; i++) {
        v->data.list.items[i] = list->data.list.items[i];
        airl_value_retain(list->data.list.items[i]);
    }
    v->data.list.items[old_len] = elem;
    airl_value_retain(elem);
    return v;
}
