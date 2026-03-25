/*
 * airl_rt_http.c - HTTP POST for the AIRL runtime
 *
 * Uses libcurl for HTTP requests. Links against -lcurl.
 */

#include "airl_rt.h"
#include <curl/curl.h>

/* ------------------------------------------------------------------ */
/*  Response buffer for curl write callback                           */
/* ------------------------------------------------------------------ */

struct ResponseBuffer {
    char  *data;
    size_t size;
};

static size_t write_callback(void *contents, size_t size, size_t nmemb, void *userp) {
    size_t realsize = size * nmemb;
    struct ResponseBuffer *buf = (struct ResponseBuffer *)userp;
    buf->data = realloc(buf->data, buf->size + realsize + 1);
    if (!buf->data) return 0;
    memcpy(buf->data + buf->size, contents, realsize);
    buf->size += realsize;
    buf->data[buf->size] = '\0';
    return realsize;
}

/* ------------------------------------------------------------------ */
/*  Helper: wrap in Ok/Err variants                                   */
/* ------------------------------------------------------------------ */

static RtValue *http_ok(RtValue *inner) {
    RtValue *tag = airl_str("Ok", 2);
    RtValue *result = airl_make_variant(tag, inner);
    airl_value_release(tag);
    return result;
}

static RtValue *http_err(const char *msg) {
    RtValue *tag = airl_str("Err", 3);
    RtValue *inner = airl_str(msg, strlen(msg));
    RtValue *result = airl_make_variant(tag, inner);
    airl_value_release(tag);
    return result;
}

/* ------------------------------------------------------------------ */
/*  http-request(method, url, body, headers) -> Result[String, String] */
/* ------------------------------------------------------------------ */

RtValue *airl_http_request(RtValue *method, RtValue *url, RtValue *body, RtValue *headers) {
    if (method->tag != RT_STR) {
        return http_err("http-request: method must be string");
    }
    if (url->tag != RT_STR) {
        return http_err("http-request: url must be string");
    }
    if (body->tag != RT_STR) {
        return http_err("http-request: body must be string");
    }
    if (headers->tag != RT_MAP) {
        return http_err("http-request: headers must be map");
    }

    /* Null-terminate strings */
    char *curl_method = malloc(method->data.s.len + 1);
    memcpy(curl_method, method->data.s.ptr, method->data.s.len);
    curl_method[method->data.s.len] = '\0';

    char *curl_url = malloc(url->data.s.len + 1);
    memcpy(curl_url, url->data.s.ptr, url->data.s.len);
    curl_url[url->data.s.len] = '\0';

    char *curl_body = malloc(body->data.s.len + 1);
    memcpy(curl_body, body->data.s.ptr, body->data.s.len);
    curl_body[body->data.s.len] = '\0';

    CURL *curl = curl_easy_init();
    if (!curl) {
        free(curl_method);
        free(curl_url);
        free(curl_body);
        return http_err("http-request: curl_easy_init failed");
    }

    /* Set up response buffer */
    struct ResponseBuffer response;
    response.data = malloc(1);
    response.data[0] = '\0';
    response.size = 0;

    /* Configure curl */
    curl_easy_setopt(curl, CURLOPT_URL, curl_url);
    curl_easy_setopt(curl, CURLOPT_CUSTOMREQUEST, curl_method);
    curl_easy_setopt(curl, CURLOPT_WRITEFUNCTION, write_callback);
    curl_easy_setopt(curl, CURLOPT_WRITEDATA, &response);
    curl_easy_setopt(curl, CURLOPT_TIMEOUT, 300L);

    /* Set body for methods that use it */
    if (body->data.s.len > 0) {
        curl_easy_setopt(curl, CURLOPT_POSTFIELDS, curl_body);
        curl_easy_setopt(curl, CURLOPT_POSTFIELDSIZE, (long)body->data.s.len);
    }

    /* Build headers from map */
    struct curl_slist *header_list = NULL;
    size_t i;
    for (i = 0; i < headers->data.map.capacity; i++) {
        MapEntry *e = &headers->data.map.entries[i];
        if (!e->occupied || e->deleted) continue;
        if (e->value->tag != RT_STR) continue;

        size_t hlen = e->key_len + 2 + e->value->data.s.len + 1;
        char *hdr = malloc(hlen);
        memcpy(hdr, e->key, e->key_len);
        hdr[e->key_len] = ':';
        hdr[e->key_len + 1] = ' ';
        memcpy(hdr + e->key_len + 2, e->value->data.s.ptr, e->value->data.s.len);
        hdr[hlen - 1] = '\0';
        header_list = curl_slist_append(header_list, hdr);
        free(hdr);
    }
    if (header_list) {
        curl_easy_setopt(curl, CURLOPT_HTTPHEADER, header_list);
    }

    /* Perform request */
    CURLcode res = curl_easy_perform(curl);

    /* Clean up */
    if (header_list) curl_slist_free_all(header_list);
    curl_easy_cleanup(curl);
    free(curl_method);
    free(curl_url);
    free(curl_body);

    if (res != CURLE_OK) {
        const char *err_msg = curl_easy_strerror(res);
        RtValue *result = http_err(err_msg);
        free(response.data);
        return result;
    }

    RtValue *resp_val = airl_str(response.data, response.size);
    free(response.data);
    return http_ok(resp_val);
}
