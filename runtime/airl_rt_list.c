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
    v->data.list.offset = 0;
    v->data.list.parent = NULL;
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
    RtValue* h = list->data.list.items[list->data.list.offset];
    airl_value_retain(h);
    return h;
}

RtValue* airl_tail(RtValue* list) {
    if (!list || list->tag != RT_LIST || list->data.list.len == 0) {
        fprintf(stderr, "airl_tail: empty list\n");
        exit(1);
    }
    size_t new_len = list->data.list.len - 1;

    /* COW tail: create a view sharing the backing array */
    RtValue* v = (RtValue*)malloc(sizeof(RtValue));
    if (!v) {
        fprintf(stderr, "airl_rt: out of memory\n");
        exit(1);
    }
    v->tag = RT_LIST;
    v->rc = 1;
    memset(&v->data, 0, sizeof(v->data));
    v->data.list.items = list->data.list.items;
    v->data.list.offset = list->data.list.offset + 1;
    v->data.list.len = new_len;
    v->data.list.cap = list->data.list.cap;

    /* Find the root owner: if list is itself a view, chain to its parent */
    RtValue* root = list->data.list.parent ? list->data.list.parent : list;
    v->data.list.parent = root;
    airl_value_retain(root);

    /* Retain each element visible in this view so they stay alive if parent
       releases them via some other path (not strictly needed with parent
       retain, but keeps the contract simple and safe). */
    /* Actually — the parent retain is sufficient: the parent owns all items
       in the backing array, so as long as parent is alive, items are alive.
       No per-element retain needed. */

    return v;
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
    v->data.list.offset = 0;
    v->data.list.parent = NULL;
    v->data.list.items = (RtValue**)malloc(new_len * sizeof(RtValue*));
    if (!v->data.list.items) {
        fprintf(stderr, "airl_rt: out of memory\n");
        exit(1);
    }

    v->data.list.items[0] = elem;
    airl_value_retain(elem);
    size_t src_offset = list->data.list.offset;
    for (size_t i = 0; i < old_len; i++) {
        v->data.list.items[i + 1] = list->data.list.items[src_offset + i];
        airl_value_retain(list->data.list.items[src_offset + i]);
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
        case RT_STR: {
            /* Return character count (Unicode codepoints), not byte length */
            size_t count = 0;
            size_t i = 0;
            while (i < v->data.s.len) {
                unsigned char byte = (unsigned char)v->data.s.ptr[i];
                if (byte < 0x80) i += 1;
                else if ((byte & 0xE0) == 0xC0) i += 2;
                else if ((byte & 0xF0) == 0xE0) i += 3;
                else if ((byte & 0xF8) == 0xF0) i += 4;
                else i += 1;
                count++;
            }
            return airl_int((int64_t)count);
        }
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
    RtValue* item = list->data.list.items[list->data.list.offset + idx];
    airl_value_retain(item);
    return item;
}

RtValue* airl_append(RtValue* list, RtValue* elem) {
    if (!list || list->tag != RT_LIST) {
        fprintf(stderr, "airl_append: not a list\n");
        exit(1);
    }

    /* In-place append when we are the sole owner and not a view */
    if (list->rc == 1 && list->data.list.parent == NULL && list->data.list.offset == 0) {
        if (list->data.list.len < list->data.list.cap) {
            /* Space available — append in place */
            list->data.list.items[list->data.list.len] = elem;
            airl_value_retain(elem);
            list->data.list.len++;
            airl_value_retain(list); /* caller expects a new ref */
            return list;
        } else {
            /* Need to grow */
            size_t new_cap = list->data.list.cap * 2;
            if (new_cap < 8) new_cap = 8;
            RtValue** new_items = (RtValue**)realloc(list->data.list.items, new_cap * sizeof(RtValue*));
            if (!new_items) {
                fprintf(stderr, "airl_rt: out of memory\n");
                exit(1);
            }
            list->data.list.items = new_items;
            list->data.list.cap = new_cap;
            list->data.list.items[list->data.list.len] = elem;
            airl_value_retain(elem);
            list->data.list.len++;
            airl_value_retain(list);
            return list;
        }
    }

    /* Shared or view — must copy */
    size_t old_len = list->data.list.len;
    size_t new_len = old_len + 1;
    size_t src_offset = list->data.list.offset;

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
    v->data.list.offset = 0;
    v->data.list.parent = NULL;
    v->data.list.items = (RtValue**)malloc(new_len * sizeof(RtValue*));
    if (!v->data.list.items) {
        fprintf(stderr, "airl_rt: out of memory\n");
        exit(1);
    }

    for (size_t i = 0; i < old_len; i++) {
        v->data.list.items[i] = list->data.list.items[src_offset + i];
        airl_value_retain(list->data.list.items[src_offset + i]);
    }
    v->data.list.items[old_len] = elem;
    airl_value_retain(elem);
    return v;
}
