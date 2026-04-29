SHELL:=/usr/bin/env bash
BINARY_NAME:=alfred-eudic

.PHONY: all build build-multiple-arch run clean
all: build run

build:
	cargo build --release
build-multi-arch:
	cargo build --release --target aarch64-apple-darwin
	cargo build --release --target x86_64-apple-darwin
	lipo -create -output "target/release/$(BINARY_NAME)" "target/aarch64-apple-darwin/release/$(BINARY_NAME)" "target/x86_64-apple-darwin/release/$(BINARY_NAME)"

run:
	cargo run -- search example
clean:
	@rm -rf rs/target
a:
	@echo "a is $$0"
b:
	@echo "b is $$0"
