.PHONY: all build release test test-all check fmt clippy clean install \
	musl musl-x86_64 musl-aarch64 gnu gnu-x86_64 gnu-aarch64 \
	cross-x86_64 cross-aarch64 release-all docker ci packages deb rpm apk \
	check-aarch64-toolchain install-man

TARGET_DIR ?= target
VERSION ?= $(shell grep '^version =' Cargo.toml | head -1 | sed 's/.*"\([^"]*\)".*/\1/')
AARCH64_LINKER ?= aarch64-linux-gnu-gcc

all: build

build:
	cargo build

release:
	cargo build --release

test:
	$(MAKE) test-all

test-all:
	cargo test --all-targets
	cargo test

check:
	cargo check --all-targets

fmt:
	cargo fmt

clippy:
	cargo clippy -- -D warnings

clean:
	cargo clean

install:
	cargo install --path .

musl: musl-x86_64

gnu: gnu-x86_64

musl-x86_64:
	rustup target add x86_64-unknown-linux-musl
	cargo build --release --target x86_64-unknown-linux-musl
	@mkdir -p dist
	cp target/x86_64-unknown-linux-musl/release/uentry dist/uentry-$(VERSION)-x86_64-musl

musl-aarch64:
	rustup target add aarch64-unknown-linux-musl
	@if command -v cross >/dev/null 2>&1; then \
		cross build --release --target aarch64-unknown-linux-musl; \
	else \
		$(MAKE) check-aarch64-toolchain && \
		CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER=$(AARCH64_LINKER) cargo build --release --target aarch64-unknown-linux-musl; \
	fi
	@mkdir -p dist
	cp target/aarch64-unknown-linux-musl/release/uentry dist/uentry-$(VERSION)-aarch64-musl

gnu-x86_64:
	rustup target add x86_64-unknown-linux-gnu
	cargo build --release --target x86_64-unknown-linux-gnu
	@mkdir -p dist
	cp target/x86_64-unknown-linux-gnu/release/uentry dist/uentry-$(VERSION)-x86_64-gnu

gnu-aarch64:
	rustup target add aarch64-unknown-linux-gnu
	@if command -v cross >/dev/null 2>&1; then \
		cross build --release --target aarch64-unknown-linux-gnu; \
	else \
		$(MAKE) check-aarch64-toolchain && \
		CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=$(AARCH64_LINKER) cargo build --release --target aarch64-unknown-linux-gnu; \
	fi
	@mkdir -p dist
	cp target/aarch64-unknown-linux-gnu/release/uentry dist/uentry-$(VERSION)-aarch64-gnu

check-aarch64-toolchain:
	@if ! command -v $(AARCH64_LINKER) >/dev/null 2>&1; then \
		echo "Missing cross linker: $(AARCH64_LINKER)"; \
		echo "Install either:"; \
		echo "  1) cross: cargo install cross --locked"; \
		echo "  2) gcc linker: sudo apt-get install -y gcc-aarch64-linux-gnu"; \
		exit 1; \
	fi

cross-x86_64:
	cross build --release --target x86_64-unknown-linux-musl
	@mkdir -p dist
	cp target/x86_64-unknown-linux-musl/release/uentry dist/uentry-$(VERSION)-x86_64-musl

cross-aarch64:
	cross build --release --target aarch64-unknown-linux-musl
	@mkdir -p dist
	cp target/aarch64-unknown-linux-musl/release/uentry dist/uentry-$(VERSION)-aarch64-musl

release-all: test clippy
	@echo "Running Make target release-all (use: make release-all)"
	@rm -rf dist
	@mkdir -p dist
	$(MAKE) musl-x86_64 musl-aarch64 gnu-x86_64 gnu-aarch64
	cd dist && sha256sum uentry-$(VERSION)-* > sha256sums.txt

docker:
	docker build -t uentry:$(VERSION) .

ci: fmt clippy test
	cargo build --release --target x86_64-unknown-linux-musl
	cargo build --release --target x86_64-unknown-linux-gnu

packages: deb rpm apk

deb: gnu-x86_64
	@which nfpm > /dev/null || (echo "Install nfpm: go install github.com/goreleaser/nfpm/v2/cmd/nfpm@latest" && exit 1)
	@mkdir -p dist
	cp dist/uentry-$(VERSION)-x86_64-gnu ./uentry
	ARCH=amd64 VERSION=$(VERSION) nfpm package --packager deb --target ./dist/uentry-$(VERSION)-x86_64-gnu.deb
	@rm -f ./uentry
	@echo "Package created: dist/uentry-$(VERSION)-x86_64-gnu.deb"

rpm: gnu-x86_64
	@which nfpm > /dev/null || (echo "Install nfpm: go install github.com/goreleaser/nfpm/v2/cmd/nfpm@latest" && exit 1)
	@mkdir -p dist
	cp dist/uentry-$(VERSION)-x86_64-gnu ./uentry
	ARCH=amd64 VERSION=$(VERSION) nfpm package --packager rpm --target ./dist/uentry-$(VERSION)-x86_64-gnu.rpm
	@rm -f ./uentry
	@echo "Package created: dist/uentry-$(VERSION)-x86_64-gnu.rpm"

apk: musl-x86_64
	@which nfpm > /dev/null || (echo "Install nfpm: go install github.com/goreleaser/nfpm/v2/cmd/nfpm@latest" && exit 1)
	@mkdir -p dist
	cp dist/uentry-$(VERSION)-x86_64-musl ./uentry
	ARCH=amd64 VERSION=$(VERSION) nfpm package --packager apk --target ./dist/uentry-$(VERSION)-x86_64-musl.apk
	@rm -f ./uentry
	@echo "Package created: dist/uentry-$(VERSION)-x86_64-musl.apk"

install-man:
	@mkdir -p $(DESTDIR)/usr/share/man/man1 $(DESTDIR)/usr/share/man/man5
	@cp man/uentry.1 $(DESTDIR)/usr/share/man/man1/
	@cp man/uentry.5 $(DESTDIR)/usr/share/man/man5/
	@gzip -f $(DESTDIR)/usr/share/man/man1/uentry.1
	@gzip -f $(DESTDIR)/usr/share/man/man5/uentry.5
	@echo "Man pages installed to $(DESTDIR)/usr/share/man/"
