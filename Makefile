# AIRL Makefile
#
# Usage:
#   make verify-api-manifest          # check api-manifest.json is up to date
#   AIRTOOLS=/path/to/airtools make verify-api-manifest

AIRTOOLS ?= airtools
STDLIB   ?= stdlib
G3       ?= target-x86_64/release/airl-driver

.PHONY: verify-api-manifest
verify-api-manifest:
	$(AIRTOOLS) doc-gen \
	    --stdlib $(STDLIB) \
	    --g3 $(G3) \
	    --out /tmp/api-manifest-docs-discard \
	    --manifest /tmp/api-manifest-check.json
	diff api-manifest.json /tmp/api-manifest-check.json || \
	    (echo "ERROR: api-manifest.json is stale — regenerate with:" && \
	     echo "  airtools doc-gen --stdlib stdlib --g3 $(G3) --manifest api-manifest.json" && \
	     exit 1)
