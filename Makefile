# Lumen: one-word ways to render a picture.
#
# Builds and renders run in the shared container through `devrun`, never on the host.
# Images land in ./out (ignored by git).
#
#   make demo            the three built-in scenes at 1280x720, then open them
#   make reference       the reference scene
#   make fixed-point     the Fixed Point scene
#   make canyon          the canyon scene
#   make 4k              the reference scene at 3840x2160, 16 samples
#   make 4k-fixed-point  the Fixed Point scene at 3840x2160, 64 samples
#   make 4k-canyon       the canyon scene at 3840x2160, 16 samples
#   make views           the debug views of every built-in scene (VIEWS= to choose)
#   make test            the test suite, debug and release
#   make nondeterminism  the nondeterminism scan (ADR 0006)
#   make clippy-bans     the check that every clippy.toml ban resolves and fires (ADR 0006)
#   make help            this list
#
# Override on the command line, for example:
#   make fixed-point WIDTH=1920 HEIGHT=1080 SAMPLES=128 CPUS=18

DEVRUN  ?= $(HOME)/git/devcontainers/bin/devrun
WIDTH   ?= 1280
HEIGHT  ?= 720
SAMPLES ?= 64
SEED    ?= 0
CPUS    ?= 8
MEMORY  ?= 8g
OUT     ?= out
OPEN    ?= 1

ROOT := $(abspath $(dir $(lastword $(MAKEFILE_LIST))))
RUN   = python3 $(DEVRUN) lumen $(ROOT) --worktree --fetch --cpus $(CPUS) --memory $(MEMORY)

VIEWS  ?= normals shading-normals depth primitive-id material-id

.PHONY: views help demo reference fixed-point canyon 4k 4k-fixed-point 4k-canyon test nondeterminism clippy-bans check-devrun

help:
	@sed -n '1,/^$$/p' $(firstword $(MAKEFILE_LIST)) | sed 's/^# \{0,1\}//'

check-devrun:
	@test -f $(DEVRUN) || { echo "devrun not found at $(DEVRUN); set DEVRUN=/path/to/devcontainers/bin/devrun"; exit 1; }

# render <scene> <file stem> <width> <height> <samples>
define render
	@mkdir -p $(OUT)
	@tmp=$$(mktemp -d) && \
	$(RUN) --export $(2).png --out $$tmp -- bash -c 'mkdir -p /scratch/out && \
	  cargo run --locked --release -q -p lumen -- render --scene $(1) \
	    --width $(3) --height $(4) --samples $(5) --seed $(SEED) --out /scratch/out/$(2).png' && \
	mv $$tmp/$(2).png $(OUT)/$(2).png && rmdir $$tmp 2>/dev/null; \
	echo "wrote $(OUT)/$(2).png"
	@if [ "$(OPEN)" = 1 ] && command -v open >/dev/null 2>&1; then open $(OUT)/$(2).png; fi
endef

reference: check-devrun
	$(call render,reference,reference-$(WIDTH)x$(HEIGHT)-$(SAMPLES)spp,$(WIDTH),$(HEIGHT),$(SAMPLES))

fixed-point: check-devrun
	$(call render,fixed-point,fixed-point-$(WIDTH)x$(HEIGHT)-$(SAMPLES)spp,$(WIDTH),$(HEIGHT),$(SAMPLES))

canyon: check-devrun
	$(call render,canyon,canyon-$(WIDTH)x$(HEIGHT)-$(SAMPLES)spp,$(WIDTH),$(HEIGHT),$(SAMPLES))

demo: reference fixed-point canyon

4k: check-devrun
	$(call render,reference,reference-3840x2160-16spp,3840,2160,16)

4k-fixed-point: check-devrun
	$(call render,fixed-point,fixed-point-3840x2160-64spp,3840,2160,64)

4k-canyon: check-devrun
	$(call render,canyon,canyon-3840x2160-16spp,3840,2160,16)

# Every debug view of every built-in scene, in one container run: out/views/<scene>-<view>.png.
views: check-devrun
	@mkdir -p $(OUT)/views
	@tmp=$$(mktemp -d) && \
	$(RUN) --export views --out $$tmp -- bash -c 'mkdir -p /scratch/out/views && \
	  cargo build --locked --release -q -p lumen && \
	  for s in reference fixed-point canyon; do for v in $(VIEWS); do \
	    /target/release/lumen render --scene $$s --view $$v --width $(WIDTH) --height $(HEIGHT) \
	      --out /scratch/out/views/$$s-$$v.png > /dev/null || exit 1; done; done' && \
	mv $$tmp/views/*.png $(OUT)/views/ && rm -rf $$tmp; \
	echo "wrote $(OUT)/views"

test: check-devrun
	$(RUN) -- bash -c 'cargo test --locked --workspace && cargo test --locked --workspace --release'

nondeterminism: check-devrun
	$(RUN) -- cargo run --locked -q -p xtask -- nondeterminism

clippy-bans: check-devrun
	$(RUN) -- cargo run --locked -q -p xtask -- clippy-bans
