#include "airl_rt.h"

/* ---- Hash function (FNV-1a) ---- */

static uint64_t hash_string(const char* key, size_t len) {
    uint64_t hash = 14695981039346656037ULL;
    for (size_t i = 0; i < len; i++) {
        hash ^= (uint64_t)(unsigned char)key[i];
        hash *= 1099511628211ULL;
    }
    return hash;
}

/* ---- Constants ---- */

#define INITIAL_CAP 16
#define LOAD_FACTOR_NUM 7
#define LOAD_FACTOR_DEN 10

/* ---- Internal helpers ---- */

static RtValue* map_alloc(size_t capacity) {
    RtValue* v = malloc(sizeof(RtValue));
    if (!v) { fprintf(stderr, "airl_rt: out of memory\n"); exit(1); }
    v->tag = RT_MAP;
    v->rc = 1;
    memset(&v->data, 0, sizeof(v->data));
    v->data.map.entries = calloc(capacity, sizeof(MapEntry));
    if (!v->data.map.entries) { fprintf(stderr, "airl_rt: out of memory\n"); exit(1); }
    v->data.map.capacity = capacity;
    v->data.map.count = 0;
    return v;
}

/* Find slot for key using linear probing.
   Returns the slot index: either the existing key's slot, or a free slot. */
static size_t map_find_slot(MapEntry* entries, size_t cap, const char* key, size_t key_len) {
    size_t idx = hash_string(key, key_len) % cap;
    size_t first_deleted = (size_t)-1;
    while (entries[idx].occupied) {
        if (!entries[idx].deleted &&
            entries[idx].key_len == key_len &&
            memcmp(entries[idx].key, key, key_len) == 0) {
            return idx; /* found existing */
        }
        if (entries[idx].deleted && first_deleted == (size_t)-1) {
            first_deleted = idx;
        }
        idx = (idx + 1) % cap;
    }
    /* Return first deleted slot if we found one, otherwise the empty slot */
    return (first_deleted != (size_t)-1) ? first_deleted : idx;
}

/* Find slot for lookup only — must find exact match or empty (no reuse of deleted) */
static size_t map_lookup_slot(MapEntry* entries, size_t cap, const char* key, size_t key_len) {
    size_t idx = hash_string(key, key_len) % cap;
    while (entries[idx].occupied) {
        if (!entries[idx].deleted &&
            entries[idx].key_len == key_len &&
            memcmp(entries[idx].key, key, key_len) == 0) {
            return idx; /* found */
        }
        idx = (idx + 1) % cap;
    }
    return (size_t)-1; /* not found */
}

/* Deep copy a map */
static RtValue* map_clone(RtValue* m) {
    size_t cap = m->data.map.capacity;
    RtValue* new_map = map_alloc(cap);
    new_map->data.map.count = m->data.map.count;
    for (size_t i = 0; i < cap; i++) {
        if (m->data.map.entries[i].occupied && !m->data.map.entries[i].deleted) {
            MapEntry* e = &new_map->data.map.entries[i];
            MapEntry* src = &m->data.map.entries[i];
            e->key = malloc(src->key_len + 1);
            memcpy(e->key, src->key, src->key_len);
            e->key[src->key_len] = '\0';
            e->key_len = src->key_len;
            e->value = src->value;
            airl_value_retain(e->value);
            e->occupied = true;
            e->deleted = false;
        }
    }
    return new_map;
}

/* Grow the hash table by doubling capacity */
static void map_grow(RtValue* m) {
    size_t new_cap = m->data.map.capacity * 2;
    MapEntry* new_entries = calloc(new_cap, sizeof(MapEntry));
    if (!new_entries) { fprintf(stderr, "airl_rt: out of memory\n"); exit(1); }
    for (size_t i = 0; i < m->data.map.capacity; i++) {
        if (m->data.map.entries[i].occupied && !m->data.map.entries[i].deleted) {
            size_t slot = map_find_slot(new_entries, new_cap,
                m->data.map.entries[i].key, m->data.map.entries[i].key_len);
            new_entries[slot] = m->data.map.entries[i];
        }
    }
    free(m->data.map.entries);
    m->data.map.entries = new_entries;
    m->data.map.capacity = new_cap;
}

/* Insert a key-value into a map (mutates the map, used on freshly cloned maps) */
static void map_insert(RtValue* m, const char* key, size_t key_len, RtValue* value) {
    /* Check load factor before insert */
    if ((m->data.map.count + 1) * LOAD_FACTOR_DEN > m->data.map.capacity * LOAD_FACTOR_NUM) {
        map_grow(m);
    }
    size_t slot = map_find_slot(m->data.map.entries, m->data.map.capacity, key, key_len);
    MapEntry* e = &m->data.map.entries[slot];
    if (e->occupied && !e->deleted) {
        /* Overwrite existing — release old value, retain new */
        airl_value_release(e->value);
        e->value = value;
        airl_value_retain(value);
    } else {
        /* New entry */
        e->key = malloc(key_len + 1);
        memcpy(e->key, key, key_len);
        e->key[key_len] = '\0';
        e->key_len = key_len;
        e->value = value;
        airl_value_retain(value);
        e->occupied = true;
        e->deleted = false;
        m->data.map.count++;
    }
}

/* ---- Public API ---- */

RtValue* airl_map_new(void) {
    return map_alloc(INITIAL_CAP);
}

RtValue* airl_map_from(RtValue* pairs) {
    RtValue* m = map_alloc(INITIAL_CAP);
    size_t len = pairs->data.list.len;
    for (size_t i = 0; i + 1 < len; i += 2) {
        RtValue* key = pairs->data.list.items[i];
        RtValue* val = pairs->data.list.items[i + 1];
        map_insert(m, key->data.s.ptr, key->data.s.len, val);
    }
    return m;
}

RtValue* airl_map_get(RtValue* m, RtValue* key) {
    size_t slot = map_lookup_slot(m->data.map.entries, m->data.map.capacity,
                                   key->data.s.ptr, key->data.s.len);
    if (slot == (size_t)-1) {
        return airl_nil();
    }
    RtValue* v = m->data.map.entries[slot].value;
    airl_value_retain(v);
    return v;
}

RtValue* airl_map_get_or(RtValue* m, RtValue* key, RtValue* default_val) {
    size_t slot = map_lookup_slot(m->data.map.entries, m->data.map.capacity,
                                   key->data.s.ptr, key->data.s.len);
    if (slot == (size_t)-1) {
        airl_value_retain(default_val);
        return default_val;
    }
    RtValue* v = m->data.map.entries[slot].value;
    airl_value_retain(v);
    return v;
}

RtValue* airl_map_set(RtValue* m, RtValue* key, RtValue* value) {
    RtValue* new_map = map_clone(m);
    map_insert(new_map, key->data.s.ptr, key->data.s.len, value);
    return new_map;
}

RtValue* airl_map_has(RtValue* m, RtValue* key) {
    size_t slot = map_lookup_slot(m->data.map.entries, m->data.map.capacity,
                                   key->data.s.ptr, key->data.s.len);
    return airl_bool(slot != (size_t)-1 ? 1 : 0);
}

RtValue* airl_map_remove(RtValue* m, RtValue* key) {
    RtValue* new_map = map_clone(m);
    size_t slot = map_lookup_slot(new_map->data.map.entries, new_map->data.map.capacity,
                                   key->data.s.ptr, key->data.s.len);
    if (slot != (size_t)-1) {
        MapEntry* e = &new_map->data.map.entries[slot];
        free(e->key);
        e->key = NULL;
        e->key_len = 0;
        airl_value_release(e->value);
        e->value = NULL;
        e->deleted = true;
        new_map->data.map.count--;
    }
    return new_map;
}

RtValue* airl_map_keys(RtValue* m) {
    size_t count = m->data.map.count;
    if (count == 0) {
        return airl_list_new(NULL, 0);
    }
    /* Collect keys */
    char** keys = malloc(sizeof(char*) * count);
    size_t* lens = malloc(sizeof(size_t) * count);
    size_t n = 0;
    for (size_t i = 0; i < m->data.map.capacity; i++) {
        if (m->data.map.entries[i].occupied && !m->data.map.entries[i].deleted) {
            keys[n] = m->data.map.entries[i].key;
            lens[n] = m->data.map.entries[i].key_len;
            n++;
        }
    }
    /* Sort keys alphabetically */
    /* We need a parallel sort — use index array */
    size_t* indices = malloc(sizeof(size_t) * n);
    for (size_t i = 0; i < n; i++) indices[i] = i;
    /* Simple insertion sort (stable, fine for typical map sizes) */
    for (size_t i = 1; i < n; i++) {
        size_t tmp = indices[i];
        size_t j = i;
        while (j > 0 && strcmp(keys[indices[j - 1]], keys[tmp]) > 0) {
            indices[j] = indices[j - 1];
            j--;
        }
        indices[j] = tmp;
    }
    /* Build result list */
    RtValue** items = malloc(sizeof(RtValue*) * n);
    for (size_t i = 0; i < n; i++) {
        size_t idx = indices[i];
        items[i] = airl_str(keys[idx], lens[idx]);
    }
    RtValue* result = airl_list_new(items, n);
    /* Release our copies (list_new retains them) */
    for (size_t i = 0; i < n; i++) {
        airl_value_release(items[i]);
    }
    free(items);
    free(indices);
    free(keys);
    free(lens);
    return result;
}

RtValue* airl_map_values(RtValue* m) {
    size_t count = m->data.map.count;
    if (count == 0) {
        return airl_list_new(NULL, 0);
    }
    /* Collect keys and values */
    char** keys = malloc(sizeof(char*) * count);
    RtValue** vals = malloc(sizeof(RtValue*) * count);
    size_t n = 0;
    for (size_t i = 0; i < m->data.map.capacity; i++) {
        if (m->data.map.entries[i].occupied && !m->data.map.entries[i].deleted) {
            keys[n] = m->data.map.entries[i].key;
            vals[n] = m->data.map.entries[i].value;
            n++;
        }
    }
    /* Sort by key, keeping vals in sync */
    for (size_t i = 1; i < n; i++) {
        char* tk = keys[i];
        RtValue* tv = vals[i];
        size_t j = i;
        while (j > 0 && strcmp(keys[j - 1], tk) > 0) {
            keys[j] = keys[j - 1];
            vals[j] = vals[j - 1];
            j--;
        }
        keys[j] = tk;
        vals[j] = tv;
    }
    /* Build result list — retain each value */
    RtValue** items = malloc(sizeof(RtValue*) * n);
    for (size_t i = 0; i < n; i++) {
        airl_value_retain(vals[i]);
        items[i] = vals[i];
    }
    RtValue* result = airl_list_new(items, n);
    /* Release our copies */
    for (size_t i = 0; i < n; i++) {
        airl_value_release(items[i]);
    }
    free(items);
    free(keys);
    free(vals);
    return result;
}

RtValue* airl_map_size(RtValue* m) {
    return airl_int((int64_t)m->data.map.count);
}
