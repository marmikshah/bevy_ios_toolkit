# bevy_ios_toolkit demo — desktop + iOS. `make` is the one entrypoint.
.DEFAULT_GOAL := help

PROJECT := ios/IosToolkitDemo.xcodeproj
SCHEME  := IosToolkitDemo

help: ## List targets
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) \
		| awk 'BEGIN{FS=":.*?## "}{printf "  \033[36m%-14s\033[0m %s\n", $$1, $$2}'

run: ## Run the demo on the desktop (against the built-in fakes)
	cargo run --bin demo

ios: ## Sanity-compile the iOS device staticlib (no Xcode needed)
	cargo build --lib --target aarch64-apple-ios --release

xcodeproj: ## Generate ios/IosToolkitDemo.xcodeproj from project.yml (needs xcodegen)
	cd ios && xcodegen generate

xcode: xcodeproj ## Open the generated project in Xcode
	open $(PROJECT)

simulator: xcodeproj ## Build + install + launch on a booted iOS Simulator
	cd ios && xcodebuild \
		-project IosToolkitDemo.xcodeproj \
		-scheme $(SCHEME) \
		-configuration Debug \
		-destination 'generic/platform=iOS Simulator' \
		-derivedDataPath build \
		build
	xcrun simctl install booted \
		ios/build/Build/Products/Debug-iphonesimulator/IosToolkitDemo.app
	xcrun simctl launch booted com.marmikshah.playground

clean: ## Remove cargo + iOS build artifacts
	cargo clean
	rm -rf ios/rustlib ios/build $(PROJECT)

.PHONY: help run ios xcodeproj xcode simulator clean
