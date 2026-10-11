# Common tasks. See docs/DEVELOPMENT.md for details.
.PHONY: test lint fmt smoke run desktop desktop-build install-macos windows clean

test:
	cargo test --workspace

lint:
	cargo fmt --all -- --check
	cargo clippy --workspace --all-targets -- -D warnings

fmt:
	cargo fmt --all

smoke:
	scripts/smoke.sh

run:
	cargo run -p aillies-cli -- run

desktop:
	cd apps/desktop/src-tauri && cargo tauri dev

desktop-build:
	cd apps/desktop/src-tauri && cargo tauri build

install-macos:
	scripts/install-macos.sh

windows:
	scripts/build-windows-from-linux.sh

clean:
	cargo clean
	rm -rf apps/desktop/src-tauri/target target-win apps/desktop/src-tauri/target-win
