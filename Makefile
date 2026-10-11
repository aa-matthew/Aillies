# Common tasks. See docs/DEVELOPMENT.md for details.
.PHONY: test test-core test-desktop test-ui lint lint-core lint-desktop ui-check fmt smoke run desktop desktop-build install-macos windows clean
DESKTOP := apps/desktop/src-tauri/Cargo.toml

# `test` and `lint` cover the Rust workspace AND the desktop shell (which is
# outside the workspace; see docs/DECISIONS.md #16). On Linux the desktop
# targets need the WebKitGTK packages listed in docs/DEVELOPMENT.md.
test: test-core test-desktop test-ui

test-core:
	cargo test --workspace

test-ui:
	node scripts/ui-settings-test.mjs

test-desktop:
	cargo test --manifest-path $(DESKTOP)

lint: lint-core lint-desktop ui-check

lint-core:
	cargo fmt --all -- --check
	cargo clippy --workspace --all-targets -- -D warnings

lint-desktop:
	cargo fmt --manifest-path $(DESKTOP) -- --check
	cargo clippy --manifest-path $(DESKTOP) --all-targets -- -D warnings

ui-check:
	@if command -v node >/dev/null; then for f in apps/desktop/ui/*.js; do node --check "$$f" || exit 1; done; else echo "node not found: UI syntax check required"; exit 1; fi

fmt:
	cargo fmt --all
	cargo fmt --manifest-path $(DESKTOP)

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
