/*
 * airl_rt_tcp.c - TCP socket builtins for the AIRL runtime
 *
 * Uses POSIX sockets. Handle table maps integer handles to file descriptors.
 */

#include "airl_rt.h"
#include <sys/socket.h>
#include <netinet/in.h>
#include <netdb.h>
#include <unistd.h>
#include <errno.h>
#include <sys/time.h>
#include <fcntl.h>

/* ---- Handle table ---- */

#define MAX_TCP_HANDLES 256

static int tcp_fds[MAX_TCP_HANDLES];
static int tcp_used[MAX_TCP_HANDLES];
static int next_handle = 1;
static int tcp_initialized = 0;

static void tcp_init(void) {
    if (!tcp_initialized) {
        memset(tcp_used, 0, sizeof(tcp_used));
        tcp_initialized = 1;
    }
}

static int alloc_handle(int fd) {
    tcp_init();
    for (int i = 0; i < MAX_TCP_HANDLES; i++) {
        int h = (next_handle + i) % MAX_TCP_HANDLES;
        if (h == 0) h = 1; /* skip handle 0 */
        if (!tcp_used[h]) {
            tcp_fds[h] = fd;
            tcp_used[h] = 1;
            next_handle = h + 1;
            return h;
        }
    }
    return -1; /* out of handles */
}

/* ---- Ok/Err helpers ---- */

static RtValue* tcp_ok(RtValue* inner) {
    RtValue* tag = airl_str("Ok", 2);
    RtValue* result = airl_make_variant(tag, inner);
    airl_value_release(tag);
    return result;
}

static RtValue* tcp_err(const char* msg) {
    RtValue* tag = airl_str("Err", 3);
    RtValue* inner = airl_str(msg, strlen(msg));
    RtValue* result = airl_make_variant(tag, inner);
    airl_value_release(tag);
    return result;
}

static RtValue* tcp_err_errno(const char* prefix) {
    char buf[512];
    snprintf(buf, sizeof(buf), "%s: %s", prefix, strerror(errno));
    return tcp_err(buf);
}

/* ---- tcp-connect(host, port) -> Result[Int, Str] ---- */

RtValue* airl_tcp_connect(RtValue* host, RtValue* port) {
    tcp_init();

    /* Extract host string (null-terminate) */
    char* hostname = (char*)malloc(host->data.s.len + 1);
    if (!hostname) return tcp_err("tcp-connect: out of memory");
    memcpy(hostname, host->data.s.ptr, host->data.s.len);
    hostname[host->data.s.len] = '\0';

    int portnum = (int)port->data.i;

    /* Resolve hostname */
    struct addrinfo hints, *res;
    memset(&hints, 0, sizeof(hints));
    hints.ai_family = AF_UNSPEC;
    hints.ai_socktype = SOCK_STREAM;

    char port_str[16];
    snprintf(port_str, sizeof(port_str), "%d", portnum);

    int gai = getaddrinfo(hostname, port_str, &hints, &res);
    free(hostname);
    if (gai != 0) {
        char buf[512];
        snprintf(buf, sizeof(buf), "tcp-connect: %s", gai_strerror(gai));
        return tcp_err(buf);
    }

    /* Create socket and connect */
    int fd = socket(res->ai_family, res->ai_socktype, res->ai_protocol);
    if (fd < 0) {
        freeaddrinfo(res);
        return tcp_err_errno("tcp-connect");
    }

    if (connect(fd, res->ai_addr, res->ai_addrlen) < 0) {
        freeaddrinfo(res);
        close(fd);
        return tcp_err_errno("tcp-connect");
    }
    freeaddrinfo(res);

    int handle = alloc_handle(fd);
    if (handle < 0) {
        close(fd);
        return tcp_err("tcp-connect: too many open connections");
    }

    return tcp_ok(airl_int((int64_t)handle));
}

/* ---- tcp-close(handle) -> Result[Nil, Str] ---- */

RtValue* airl_tcp_close(RtValue* handle) {
    int h = (int)handle->data.i;
    if (h < 0 || h >= MAX_TCP_HANDLES || !tcp_used[h]) {
        return tcp_err("tcp-close: invalid handle");
    }
    close(tcp_fds[h]);
    tcp_used[h] = 0;
    return tcp_ok(airl_nil());
}

/* ---- tcp-send(handle, data) -> Result[Int, Str] ---- */

RtValue* airl_tcp_send(RtValue* handle, RtValue* data) {
    int h = (int)handle->data.i;
    if (h < 0 || h >= MAX_TCP_HANDLES || !tcp_used[h]) {
        return tcp_err("tcp-send: invalid handle");
    }

    size_t len = data->data.list.len;
    size_t off = data->data.list.offset;
    uint8_t* buf = (uint8_t*)malloc(len);
    if (!buf && len > 0) return tcp_err("tcp-send: out of memory");

    for (size_t i = 0; i < len; i++) {
        buf[i] = (uint8_t)(data->data.list.items[off + i]->data.i & 0xFF);
    }

    size_t total_sent = 0;
    while (total_sent < len) {
        ssize_t n = send(tcp_fds[h], buf + total_sent, len - total_sent, 0);
        if (n < 0) {
            free(buf);
            return tcp_err_errno("tcp-send");
        }
        total_sent += (size_t)n;
    }

    free(buf);
    return tcp_ok(airl_int((int64_t)total_sent));
}

/* ---- tcp-recv(handle, max) -> Result[List, Str] ---- */

RtValue* airl_tcp_recv(RtValue* handle, RtValue* max_bytes) {
    int h = (int)handle->data.i;
    if (h < 0 || h >= MAX_TCP_HANDLES || !tcp_used[h]) {
        return tcp_err("tcp-recv: invalid handle");
    }

    size_t max = (size_t)max_bytes->data.i;
    uint8_t* buf = (uint8_t*)malloc(max);
    if (!buf) return tcp_err("tcp-recv: out of memory");

    ssize_t n = recv(tcp_fds[h], buf, max, 0);
    if (n < 0) {
        free(buf);
        return tcp_err_errno("tcp-recv");
    }

    /* Build list of ints from received bytes */
    RtValue** items = (RtValue**)malloc((size_t)n * sizeof(RtValue*));
    if (!items && n > 0) { free(buf); return tcp_err("tcp-recv: out of memory"); }
    for (ssize_t i = 0; i < n; i++) {
        items[i] = airl_int((int64_t)buf[i]);
    }
    RtValue* list = airl_list_new(items, (size_t)n);
    for (ssize_t i = 0; i < n; i++) {
        airl_value_release(items[i]);
    }
    free(items);
    free(buf);

    return tcp_ok(list);
}

/* ---- tcp-recv-exact(handle, n) -> Result[List, Str] ---- */

RtValue* airl_tcp_recv_exact(RtValue* handle, RtValue* count) {
    int h = (int)handle->data.i;
    if (h < 0 || h >= MAX_TCP_HANDLES || !tcp_used[h]) {
        return tcp_err("tcp-recv-exact: invalid handle");
    }

    size_t total = (size_t)count->data.i;
    uint8_t* buf = (uint8_t*)malloc(total);
    if (!buf) return tcp_err("tcp-recv-exact: out of memory");

    size_t received = 0;
    while (received < total) {
        ssize_t n = recv(tcp_fds[h], buf + received, total - received, 0);
        if (n <= 0) {
            free(buf);
            if (n == 0) return tcp_err("tcp-recv-exact: connection closed");
            return tcp_err_errno("tcp-recv-exact");
        }
        received += (size_t)n;
    }

    /* Build list of ints */
    RtValue** items = (RtValue**)malloc(total * sizeof(RtValue*));
    if (!items) { free(buf); return tcp_err("tcp-recv-exact: out of memory"); }
    for (size_t i = 0; i < total; i++) {
        items[i] = airl_int((int64_t)buf[i]);
    }
    RtValue* list = airl_list_new(items, total);
    for (size_t i = 0; i < total; i++) {
        airl_value_release(items[i]);
    }
    free(items);
    free(buf);

    return tcp_ok(list);
}

/* ---- tcp-set-timeout(handle, ms) -> Result[Nil, Str] ---- */

RtValue* airl_tcp_set_timeout(RtValue* handle, RtValue* ms) {
    int h = (int)handle->data.i;
    if (h < 0 || h >= MAX_TCP_HANDLES || !tcp_used[h]) {
        return tcp_err("tcp-set-timeout: invalid handle");
    }

    int64_t millis = ms->data.i;
    struct timeval tv;
    if (millis > 0) {
        tv.tv_sec = millis / 1000;
        tv.tv_usec = (millis % 1000) * 1000;
    } else {
        tv.tv_sec = 0;
        tv.tv_usec = 0;
    }

    if (setsockopt(tcp_fds[h], SOL_SOCKET, SO_RCVTIMEO, &tv, sizeof(tv)) < 0) {
        return tcp_err_errno("tcp-set-timeout (recv)");
    }
    if (setsockopt(tcp_fds[h], SOL_SOCKET, SO_SNDTIMEO, &tv, sizeof(tv)) < 0) {
        return tcp_err_errno("tcp-set-timeout (send)");
    }

    return tcp_ok(airl_nil());
}
