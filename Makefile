HAZRUST := $(shell which cargo >/dev/null && echo yes || echo no)
HAZRUSTUP := $(shell which cargo >/dev/null && echo yes || echo no)

default: build-release

ifeq ($(HAZRUST),yes)

ifeq ($(HAZRUSTUP),yes)
cargo-setup:
	rustup target add x86_64-unknown-linux-musl

cargo-bench:
	rustup run nightly cargo bench
else
cargo-setup:
	$(warning No rustup hope you have libmusl)

cargo-bench:
	$(error Must have rustup with nightly installed)
endif

cargo-clean:
	cargo clean

ec2-rs: cargo-setup
	cargo build

ec2-rs-release: cargo-setup
	cargo build --release

ec2-rs-static: cargo-setup
	cargo build --target x86_64-unknown-linux-musl

ec2-rs-static-release: cargo-setup
	cargo build --release --target x86_64-unknown-linux-musl

else
define CAN_HAZ_RUST

We need a Rust Toolchain, as well as rustup in order to compile
a static binary for nom-nom.

https://rustup.rs/

endef

ec2-rs:
	$(error $(CAN_HAZ_RUST))

ec2-rs-release:
	$(error $(CAN_HAZ_RUST))

ec2-rs-static:
	$(error $(CAN_HAZ_RUST))

ec2-rs-static-release:
	$(error $(CAN_HAZ_RUST))

cargo-clean:
	$(warning No Rust toolchain so not cleaning anything.)

cargo-setup:
	$(warning No Rustup, so not setting anything up.)

cargo-bench:
	$(warning No Rustup, so not doing anything.)
endif

build: ec2-rs
build-release: ec2-rs-release
release: ec2-rs-release
build-static: ec2-rs-static
build-static-release: ec2-rs-static-release
clean: cargo-clean
setup: cargo-setup
bench: cargo-bench

.PHONY: default build build-release build-static build-static-release clean setup release bench
