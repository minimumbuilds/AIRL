AIRL_DIR ?= ../..
G3 = $(AIRL_DIR)/g3
STDLIB = $(AIRL_DIR)/stdlib
CAIRLI = $(HOME)/repos/CairLI/src/cairli.airl

LIBS = $(CAIRLI) \
       $(STDLIB)/io.airl \
       $(STDLIB)/string.airl \
       $(STDLIB)/path.airl \
       $(STDLIB)/map.airl \
       $(STDLIB)/json.airl \
       $(STDLIB)/base64.airl

SRCS = src/lexer.airl \
       src/parser.airl \
       src/eval.airl \
       src/builtins.airl \
       src/format.airl \
       src/cli.airl \
       src/main.airl

all: build/airq

build/airq: $(SRCS)
	cd $(AIRL_DIR) && AIRL_STDLIB=$(abspath $(STDLIB)) \
	  $(abspath $(G3)) -- $(abspath $(LIBS)) $(addprefix $(CURDIR)/,$(SRCS)) \
	  -o $(CURDIR)/build/airq

test:
	cd $(AIRL_DIR) && AIRL_STDLIB=$(abspath $(STDLIB)) \
	  $(abspath $(G3)) -- $(abspath $(LIBS)) \
	  $(CURDIR)/src/lexer.airl $(CURDIR)/tests/test_lexer.airl \
	  -o /tmp/test_lexer && /tmp/test_lexer && rm -f /tmp/test_lexer
	cd $(AIRL_DIR) && AIRL_STDLIB=$(abspath $(STDLIB)) \
	  $(abspath $(G3)) -- $(abspath $(LIBS)) \
	  $(CURDIR)/src/lexer.airl $(CURDIR)/src/parser.airl \
	  $(CURDIR)/tests/test_parser.airl \
	  -o /tmp/test_parser && /tmp/test_parser && rm -f /tmp/test_parser
	cd $(AIRL_DIR) && AIRL_STDLIB=$(abspath $(STDLIB)) \
	  $(abspath $(G3)) -- $(abspath $(LIBS)) \
	  $(CURDIR)/src/lexer.airl $(CURDIR)/src/parser.airl \
	  $(CURDIR)/src/eval.airl $(CURDIR)/src/builtins.airl \
	  $(CURDIR)/tests/test_eval.airl \
	  -o /tmp/test_eval && /tmp/test_eval && rm -f /tmp/test_eval

clean:
	rm -rf build/*

.PHONY: all test clean
