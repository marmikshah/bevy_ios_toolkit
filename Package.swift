// swift-tools-version:5.9
import PackageDescription

// The Swift half of the bevy_ios_toolkit FFI contract. Each product mirrors a
// cargo feature of the Rust crate and ships the `@_cdecl` shims that the crate's
// `extern "C"` symbols resolve against at link time. Both halves are co-versioned
// through this repo's git tags — one tag publishes the crate to crates.io and
// pins the shims a consumer pulls over SPM, which is what keeps them in lockstep.
//
// A consumer adds this repo as an SPM dependency and links ONLY the products it
// ships (a game with no ads never compiles the ad code). Per-app configuration
// — `Info.plist` keys, the Game Center entitlement, ad-unit ids — stays in the
// app; the shims carry none of it.
let package = Package(
    name: "BevyIosToolkit",
    platforms: [.iOS(.v16)],
    products: [
        .library(name: "Platform", targets: ["Platform"]),
        .library(name: "Store", targets: ["Store"]),
        .library(name: "Ads", targets: ["Ads"]),
        .library(name: "Att", targets: ["Att"]),
        // Named for the integration, not the framework: a target called
        // `GameKit` would shadow Apple's `GameKit` module inside the shim.
        .library(name: "GameCenter", targets: ["GameCenter"]),
        .library(name: "Review", targets: ["Review"]),
    ],
    dependencies: [
        // Pulls in the UMP consent SDK transitively. Do NOT add a standalone
        // UserMessagingPlatform package — it collides on product naming.
        .package(
            url: "https://github.com/googleads/swift-package-manager-google-mobile-ads",
            from: "12.0.0"
        ),
    ],
    targets: [
        .target(
            name: "Platform",
            linkerSettings: [.linkedFramework("UIKit"), .linkedFramework("Foundation")]
        ),
        .target(
            name: "Store",
            linkerSettings: [.linkedFramework("StoreKit")]
        ),
        .target(
            name: "Ads",
            dependencies: [
                .product(
                    name: "GoogleMobileAds",
                    package: "swift-package-manager-google-mobile-ads"
                ),
            ],
            linkerSettings: [.linkedFramework("UIKit")]
        ),
        .target(
            name: "Att",
            linkerSettings: [
                .linkedFramework("AppTrackingTransparency"),
                .linkedFramework("AdSupport"),
            ]
        ),
        .target(
            name: "GameCenter",
            linkerSettings: [.linkedFramework("GameKit")]
        ),
        .target(
            name: "Review",
            linkerSettings: [.linkedFramework("StoreKit")]
        ),
    ]
)
