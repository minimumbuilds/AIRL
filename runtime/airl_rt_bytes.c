/*
 * airl_rt_bytes.c - Byte encoding/decoding builtins for the AIRL runtime
 *
 * Provides big-endian integer encoding, string<->bytes conversion,
 * buffer manipulation, and CRC32C checksums.
 */

#include "airl_rt.h"

/* ---- Helper: build a list of N int elements from a byte array ---- */

static RtValue* bytes_to_list(const uint8_t* data, size_t len) {
    RtValue** items = (RtValue**)malloc(len * sizeof(RtValue*));
    if (!items) { fprintf(stderr, "airl_rt: out of memory\n"); exit(1); }
    for (size_t i = 0; i < len; i++) {
        items[i] = airl_int((int64_t)data[i]);
    }
    RtValue* list = airl_list_new(items, len);
    for (size_t i = 0; i < len; i++) {
        airl_value_release(items[i]);
    }
    free(items);
    return list;
}

/* ---- Helper: extract bytes from a list of ints into a buffer ---- */

static uint8_t* list_to_bytes(RtValue* list, size_t* out_len) {
    if (!list || list->tag != RT_LIST) {
        *out_len = 0;
        return NULL;
    }
    size_t len = list->data.list.len;
    size_t off = list->data.list.offset;
    uint8_t* buf = (uint8_t*)malloc(len);
    if (!buf && len > 0) { fprintf(stderr, "airl_rt: out of memory\n"); exit(1); }
    for (size_t i = 0; i < len; i++) {
        RtValue* item = list->data.list.items[off + i];
        buf[i] = (uint8_t)(item->data.i & 0xFF);
    }
    *out_len = len;
    return buf;
}

/* ---- Big-endian integer encoding ---- */

RtValue* airl_bytes_from_int16(RtValue* n) {
    int16_t val = (int16_t)n->data.i;
    uint8_t bytes[2];
    bytes[0] = (uint8_t)((val >> 8) & 0xFF);
    bytes[1] = (uint8_t)(val & 0xFF);
    return bytes_to_list(bytes, 2);
}

RtValue* airl_bytes_from_int32(RtValue* n) {
    int32_t val = (int32_t)n->data.i;
    uint8_t bytes[4];
    bytes[0] = (uint8_t)((val >> 24) & 0xFF);
    bytes[1] = (uint8_t)((val >> 16) & 0xFF);
    bytes[2] = (uint8_t)((val >> 8) & 0xFF);
    bytes[3] = (uint8_t)(val & 0xFF);
    return bytes_to_list(bytes, 4);
}

RtValue* airl_bytes_from_int64(RtValue* n) {
    int64_t val = n->data.i;
    uint8_t bytes[8];
    for (int i = 7; i >= 0; i--) {
        bytes[7 - i] = (uint8_t)((val >> (i * 8)) & 0xFF);
    }
    return bytes_to_list(bytes, 8);
}

/* ---- Big-endian integer decoding ---- */

RtValue* airl_bytes_to_int16(RtValue* buf, RtValue* offset) {
    size_t off = (size_t)offset->data.i;
    size_t base = buf->data.list.offset;
    int16_t val = (int16_t)(
        ((int16_t)(buf->data.list.items[base + off]->data.i & 0xFF) << 8) |
        ((int16_t)(buf->data.list.items[base + off + 1]->data.i & 0xFF))
    );
    return airl_int((int64_t)val);
}

RtValue* airl_bytes_to_int32(RtValue* buf, RtValue* offset) {
    size_t off = (size_t)offset->data.i;
    size_t base = buf->data.list.offset;
    int32_t val =
        ((int32_t)(buf->data.list.items[base + off]->data.i & 0xFF) << 24) |
        ((int32_t)(buf->data.list.items[base + off + 1]->data.i & 0xFF) << 16) |
        ((int32_t)(buf->data.list.items[base + off + 2]->data.i & 0xFF) << 8) |
        ((int32_t)(buf->data.list.items[base + off + 3]->data.i & 0xFF));
    return airl_int((int64_t)val);
}

RtValue* airl_bytes_to_int64(RtValue* buf, RtValue* offset) {
    size_t off = (size_t)offset->data.i;
    size_t base = buf->data.list.offset;
    int64_t val = 0;
    for (int i = 0; i < 8; i++) {
        val = (val << 8) | (buf->data.list.items[base + off + i]->data.i & 0xFF);
    }
    return airl_int(val);
}

/* ---- String <-> bytes conversion ---- */

RtValue* airl_bytes_from_string(RtValue* s) {
    return bytes_to_list((const uint8_t*)s->data.s.ptr, s->data.s.len);
}

RtValue* airl_bytes_to_string(RtValue* buf, RtValue* offset, RtValue* len) {
    size_t off = (size_t)offset->data.i;
    size_t slen = (size_t)len->data.i;
    size_t base = buf->data.list.offset;

    char* str = (char*)malloc(slen + 1);
    if (!str) { fprintf(stderr, "airl_rt: out of memory\n"); exit(1); }
    for (size_t i = 0; i < slen; i++) {
        str[i] = (char)(buf->data.list.items[base + off + i]->data.i & 0xFF);
    }
    str[slen] = '\0';
    RtValue* result = airl_str(str, slen);
    free(str);
    return result;
}

/* ---- Buffer manipulation ---- */

RtValue* airl_bytes_concat(RtValue* a, RtValue* b) {
    size_t a_len = a->data.list.len;
    size_t b_len = b->data.list.len;
    size_t total = a_len + b_len;
    size_t a_off = a->data.list.offset;
    size_t b_off = b->data.list.offset;

    RtValue** items = (RtValue**)malloc(total * sizeof(RtValue*));
    if (!items && total > 0) { fprintf(stderr, "airl_rt: out of memory\n"); exit(1); }

    for (size_t i = 0; i < a_len; i++) {
        items[i] = a->data.list.items[a_off + i];
    }
    for (size_t i = 0; i < b_len; i++) {
        items[a_len + i] = b->data.list.items[b_off + i];
    }

    RtValue* result = airl_list_new(items, total);
    free(items);
    return result;
}

RtValue* airl_bytes_slice(RtValue* buf, RtValue* offset, RtValue* len) {
    size_t off = (size_t)offset->data.i;
    size_t slen = (size_t)len->data.i;
    size_t base = buf->data.list.offset;

    RtValue** items = (RtValue**)malloc(slen * sizeof(RtValue*));
    if (!items && slen > 0) { fprintf(stderr, "airl_rt: out of memory\n"); exit(1); }

    for (size_t i = 0; i < slen; i++) {
        items[i] = buf->data.list.items[base + off + i];
    }

    RtValue* result = airl_list_new(items, slen);
    free(items);
    return result;
}

/* ---- CRC32C (Castagnoli) ---- */

static const uint32_t crc32c_table[256] = {
    0x00000000, 0xF26B8303, 0xE13B70F7, 0x1350F3F4, 0xC79A971F, 0x35F1141C, 0x26A1E7E8, 0xD4CA64EB,
    0x8AD958CF, 0x78B2DBCC, 0x6BE22838, 0x9989AB3B, 0x4D43CFD0, 0xBF284CD3, 0xAC78BF27, 0x5E133C24,
    0x105EC76F, 0xE235446C, 0xF165B798, 0x030E349B, 0xD7C45070, 0x25AFD373, 0x36FF2087, 0xC494A384,
    0x9A879FA0, 0x68EC1CA3, 0x7BBCEF57, 0x89D76C54, 0x5D1D08BF, 0xAF768BBC, 0xBC267848, 0x4E4DFB4B,
    0x20BD8EDE, 0xD2D60DDD, 0xC186FE29, 0x33ED7D2A, 0xE72719C1, 0x154C9AC2, 0x061C6936, 0xF477EA35,
    0xAA64D611, 0x580F5512, 0x4B5FA6E6, 0xB93425E5, 0x6DFE410E, 0x9F95C20D, 0x8CC531F9, 0x7EAEB2FA,
    0x30E349B1, 0xC288CAB2, 0xD1D83946, 0x23B3BA45, 0xF779DEAE, 0x05125DAD, 0x1642AE59, 0xE4292D5A,
    0xBA3A117E, 0x4851927D, 0x5B016189, 0xA96AE28A, 0x7DA08661, 0x8FCB0562, 0x9C9BF696, 0x6EF07595,
    0x417B1DBC, 0xB3109EBF, 0xA0406D4B, 0x5228EE48, 0x86E68AA3, 0x74AD09A0, 0x67FDFA54, 0x95967957,
    0xCB854573, 0x39EEC670, 0x2ABE3584, 0xD8D5B687, 0x0C1FD26C, 0xFE74516F, 0xED24A29B, 0x1F4F2198,
    0x5102DAD3, 0xA36959D0, 0xB039AA24, 0x42522927, 0x96984DCC, 0x64F3CECF, 0x77A33D3B, 0x85C8BE38,
    0xDBDB821C, 0x29B0011F, 0x3AE0F2EB, 0xC88B71E8, 0x1C411503, 0xEE2A9600, 0xFD7A65F4, 0x0F11E6F7,
    0x2F8AD6D6, 0xDDE155D5, 0xCEB10621, 0x3CDA8522, 0xE810E1C9, 0x1A7B62CA, 0x092B913E, 0xFB40123D,
    0xA5532E19, 0x5738AD1A, 0x44685EEE, 0xB603DDED, 0x62C9B906, 0x90A23A05, 0x83F2C9F1, 0x71994AF2,
    0x3FD4B1B9, 0xCDBF32BA, 0xDEEFC14E, 0x2C84424D, 0xF84E26A6, 0x0A25A5A5, 0x19755651, 0xEB1ED552,
    0xB50DE976, 0x47666A75, 0x54369981, 0xA65D1A82, 0x72977E69, 0x80FCFD6A, 0x93AC0E9E, 0x61C78D9D,
    0x4ED8BADB, 0xBCB339D8, 0xAFE3CA2C, 0x5D88492F, 0x89422DC4, 0x7B29AEC7, 0x68795D33, 0x9A12DE30,
    0xC401E214, 0x366A6117, 0x253A92E3, 0xD75111E0, 0x039B750B, 0xF1F0F608, 0xE2A005FC, 0x10CB86FF,
    0x5E8673B4, 0xACEDF0B7, 0xBFBD0343, 0x4DD68040, 0x991CE4AB, 0x6B7767A8, 0x7827945C, 0x8A4C175F,
    0xD45F2B7B, 0x2634A878, 0x35645B8C, 0xC70FD88F, 0x13C5BC64, 0xE1AE3F67, 0xF2FECC93, 0x00954F90,
    0x6E6F0DEB, 0x9C048EE8, 0x8F547D1C, 0x7D3FFE1F, 0xA9F59AF4, 0x5B9E19F7, 0x48CEEA03, 0xBAA56900,
    0xE4B65524, 0x16DDD627, 0x058D25D3, 0xF7E6A6D0, 0x232CC23B, 0xD1474138, 0xC217B2CC, 0x307C31CF,
    0x7E31CA84, 0x8C5A4987, 0x9F0ABA73, 0x6D613970, 0xB9AB5D9B, 0x4BC0DE98, 0x58902D6C, 0xAAFBAE6F,
    0xF4E8924B, 0x06831148, 0x15D3E2BC, 0xE7B861BF, 0x33720554, 0xC1198657, 0xD24975A3, 0x2022F6A0,
    0x9FADA3E1, 0x6DC620E2, 0x7E96D316, 0x8CFD5015, 0x583734FE, 0xAA5CB7FD, 0xB90C4409, 0x4B67C70A,
    0x1574FB2E, 0xE71F782D, 0xF44F8BD9, 0x062408DA, 0xD2EE6C31, 0x2085EF32, 0x33D51CC6, 0xC1BE9FC5,
    0x8FF3648E, 0x7D98E78D, 0x6EC81479, 0x9CA3977A, 0x4869F391, 0xBA027092, 0xA9528366, 0x5B390065,
    0x052A3C41, 0xF741BF42, 0xE4114CB6, 0x167ACFB5, 0xC2B0AB5E, 0x30DB285D, 0x238BDBA9, 0xD1E058AA,
    0xFF7B3085, 0x0D10B386, 0x1E404072, 0xEC2BC371, 0x38E1A79A, 0xCA8A2499, 0xD9DAD76D, 0x2BB1546E,
    0x75A2684A, 0x87C9EB49, 0x949918BD, 0x66F29BBE, 0xB238FF55, 0x40537C56, 0x53038FA2, 0xA1680CA1,
    0xEF25F7EA, 0x1D4E74E9, 0x0E1E871D, 0xFC75041E, 0x28BF60F5, 0xDAD4E3F6, 0xC9841002, 0x3BEF9301,
    0x65FCAF25, 0x97972C26, 0x84C7DFD2, 0x76AC5CD1, 0xA266383A, 0x500DBB39, 0x435D48CD, 0xB136CBCE,
};

RtValue* airl_crc32c(RtValue* buf) {
    size_t len;
    uint8_t* data = list_to_bytes(buf, &len);
    uint32_t crc = 0xFFFFFFFF;
    for (size_t i = 0; i < len; i++) {
        crc = crc32c_table[(crc ^ data[i]) & 0xFF] ^ (crc >> 8);
    }
    crc ^= 0xFFFFFFFF;
    free(data);
    return airl_int((int64_t)crc);
}
