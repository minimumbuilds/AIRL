AIRL_DIR ?= ../..
G3 = $(AIRL_DIR)/g3
STDLIB = $(AIRL_DIR)/stdlib
CAIRLI = $(HOME)/repos/CairLI/src/cairli.airl

LIBS = $(CAIRLI) \
       $(STDLIB)/io.airl \
       $(STDLIB)/string.airl \
       $(STDLIB)/path.airl \
       $(STDLIB)/map.airl

SRCS = src/glob.airl \
       src/ignore.airl \
       src/types.airl \
       src/walk.airl \
       src/search.airl \
       src/format.airl \
       src/cli.airl \
       src/main.airl

all: build/airgrep

build/airgrep: $(SRCS)
	cd $(AIRL_DIR) && AIRL_STDLIB=$(abspath $(STDLIB)) \
	  $(abspath $(G3)) -- $(abspath $(LIBS)) $(addprefix $(CURDIR)/,$(SRCS)) \
	  -o $(CURDIR)/build/airgrep

test:
	cd $(AIRL_DIR) && AIRL_STDLIB=$(abspath $(STDLIB)) \
	  $(abspath $(G3)) -- $(abspath $(LIBS)) \
	  $(CURDIR)/src/glob.airl $(CURDIR)/tests/test_glob.airl \
	  -o /tmp/test_glob && /tmp/test_glob && rm -f /tmp/test_glob
	cd $(AIRL_DIR) && AIRL_STDLIB=$(abspath $(STDLIB)) \
	  $(abspath $(G3)) -- $(abspath $(LIBS)) \
	  $(CURDIR)/src/glob.airl $(CURDIR)/src/ignore.airl \
	  $(CURDIR)/tests/test_ignore.airl \
	  -o /tmp/test_ignore && /tmp/test_ignore && rm -f /tmp/test_ignore
	cd $(AIRL_DIR) && AIRL_STDLIB=$(abspath $(STDLIB)) \
	  $(abspath $(G3)) -- $(abspath $(LIBS)) \
	  $(CURDIR)/src/glob.airl $(CURDIR)/src/ignore.airl \
	  $(CURDIR)/src/types.airl $(CURDIR)/src/walk.airl \
	  $(CURDIR)/src/search.airl \
	  $(CURDIR)/tests/test_search.airl \
	  -o /tmp/test_search && /tmp/test_search && rm -f /tmp/test_search

clean:
	rm -rf build/*

.PHONY: all test clean
