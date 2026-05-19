SHELL:=/usr/bin/env bash
BINARY_NAME:=alfred-eudic
# Alfred invokes `./bin/alfred-eudic` per info.plist — the installed
# workflow's `bin/` subdir, NOT the workflow root. Always deploy here.
INSTALL_DIR:=$(HOME)/Library/Application Support/Alfred/Alfred.alfredpreferences/workflows/user.workflow.4D4E31FF-94A3-4DB7-87CE-ACB783925B51

.PHONY: all build build-multi-arch deploy run clean
all: build run

build:
	cargo build --release

deploy: build
	cp target/release/$(BINARY_NAME) "$(INSTALL_DIR)/bin/$(BINARY_NAME)"
	@echo "deployed → $(INSTALL_DIR)/bin/$(BINARY_NAME)"
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
