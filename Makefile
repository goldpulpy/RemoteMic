APP     := remotemic
TARGET  := x86_64-unknown-linux-musl
RELEASE := target/$(TARGET)/release/$(APP)

CPU_TARGET := generic

RUSTFLAGS := \
    -C target-cpu=$(CPU_TARGET) \
    -C target-feature=+crt-static \
    -C link-arg=-Wl,-z,noexecstack \
    -C link-arg=-Wl,--build-id=none \
    -C link-arg=-Wl,-z,relro \
    -C link-arg=-Wl,-z,now \
    -C link-arg=-Wl,-z,separate-code \
    -C link-arg=-Wl,--gc-sections \
    -C strip=none

GREEN := \033[0;32m
YELLOW := \033[0;33m
RED := \033[0;31m
NC := \033[0m # No Color

.DEFAULT_GOAL := help

.PHONY: release
release:
	RUSTFLAGS="$(RUSTFLAGS)" cargo build --release --target $(TARGET)
	objcopy --strip-all --remove-section=.comment --remove-section=.note $(RELEASE) $(RELEASE)
	@echo "${GREEN}Built: $(RELEASE)${NC}"

.PHONY: security
security:
	@command -v checksec >/dev/null || { echo "${RED}checksec not found: apt install checksec${NC}"; exit 1; }
	checksec --file=$(RELEASE)
	file $(RELEASE)

.PHONY: lint
lint:
	cargo clippy --all-targets --all-features -- -D warnings
	@echo "${GREEN}Linted${NC}"

.PHONY: format
format:
	cargo fmt --all
	@echo "${GREEN}Formatted${NC}"

.PHONY: audit
audit:
	cargo audit
	@echo "${GREEN}Audited${NC}"

.PHONY: clean
clean:
	cargo clean
	@echo "${GREEN}Cleaned${NC}"

.PHONY: help
help:
	@echo "${GREEN}Build:${NC}"
	@echo "  make release         Build release binary"
	@echo "  make security        Run checksec on release binary"
	@echo ""
	@echo "${YELLOW}Development:${NC}"
	@echo "  make format          Format project"
	@echo "  make lint            Run clippy"
	@echo "  make audit           Check dependencies for CVEs"
	@echo "  make clean           Clean project"
	@echo ""
	@echo "Environment:"
	@echo "  APP=$(APP)"
	@echo "  TARGET=$(TARGET)"
	@echo "  CPU_TARGET=$(CPU_TARGET)"

