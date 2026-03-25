CC ?= cc
CFLAGS = -O2 -Wall -Wextra -std=c99 -D_POSIX_C_SOURCE=200809L

SRCS = airl_rt.c airl_rt_list.c airl_rt_string.c airl_rt_map.c airl_rt_variant.c airl_rt_json.c airl_rt_process.c airl_rt_http.c airl_rt_bytes.c airl_rt_tcp.c airl_rt_misc.c
OBJS = $(SRCS:.c=.o)

libairl_rt_c.a: $(OBJS)
	ar rcs $@ $^

%.o: %.c airl_rt.h
	$(CC) $(CFLAGS) -c $< -o $@

test: test_rt
	./test_rt

test_rt: test_rt.c $(OBJS)
	$(CC) $(CFLAGS) -o $@ $^ -lm -lcurl

clean:
	rm -f *.o libairl_rt_c.a test_rt

.PHONY: clean test
