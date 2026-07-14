# truemail - команды разработки
# Старые build-артефакты чистятся cargo-sweep, актуальный кэш сохраняется.

.PHONY: dev dev-check build migrate-new lint fmt test clean sweep sweep-preview setup

ifeq ($(OS),Windows_NT)
DEV_CMD = pwsh -NoProfile -ExecutionPolicy Bypass -File scripts/dev.ps1
DEV_CHECK_CMD = pwsh -NoProfile -ExecutionPolicy Bypass -File scripts/dev.ps1 -Check
BUILD_CMD = pwsh -NoProfile -ExecutionPolicy Bypass -File scripts/with-perl.ps1 -WorkingDirectory apps/desktop/src-tauri cargo tauri build
TEST_CMD = pwsh -NoProfile -ExecutionPolicy Bypass -File scripts/with-perl.ps1 cargo test --workspace --all-targets
SETUP_PERL = pwsh -NoProfile -ExecutionPolicy Bypass -File scripts/ensure-perl.ps1
else
DEV_CMD = sh scripts/dev.sh
DEV_CHECK_CMD = sh scripts/dev.sh --check
BUILD_CMD = cd apps/desktop/src-tauri && cargo tauri build
TEST_CMD = cargo test --workspace --all-targets
SETUP_PERL = perl -v >/dev/null
endif

# Запуск десктоп-приложения в режиме разработки (Tauri v2)
dev:
	@$(DEV_CMD)

# Быстрая проверка Windows dev-обвязки без запуска приложения.
dev-check:
	@$(DEV_CHECK_CMD)

# Установка инструментов разработки
setup:
	$(SETUP_PERL)
	cargo install tauri-cli --version "^2" --locked
	cargo install sqlx-cli --no-default-features --features sqlite --locked
	cargo install cargo-sweep --version "0.8.0" --locked

# Удалить только build-артефакты, которыми не пользовались 30 дней.
sweep:
	cargo sweep --time 30 .

# Показать, что будет удалено, не меняя файлы.
sweep-preview:
	cargo sweep --dry-run --time 30 .

# Создать новую миграцию: make migrate-new name=add_something
migrate-new:
	cd crates/core && sqlx migrate add $(name)

# Сборка релиза десктоп-приложения
build:
	@$(BUILD_CMD)

fmt:
	cargo fmt --all

lint:
	cargo fmt --all -- --check
	cargo clippy --workspace --all-targets -- -D warnings

test:
	@$(TEST_CMD)

clean:
	cargo clean
