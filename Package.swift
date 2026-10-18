// swift-tools-version:5.9
import PackageDescription

// The Swift half of bevy_ios_toolkit: one product per cargo feature, holding
// the `@_cdecl` shims its `extern "C"` symbols link against. Pin it to the same
// Git tag as the crate, and link only the products you ship.
let package = Package(
    name: "BevyIosToolkit",
    platforms: [.iOS("26.0")],
    products: [
        .library(name: "Platform", targets: ["Platform"]),
        .library(name: "Store", targets: ["Store"]),
        .library(name: "Ads", targets: ["Ads"]),
        .library(name: "Att", targets: ["Att"]),
        // Named for the integration, not the framework: a target called
        // `GameKit` would shadow Apple's `GameKit` module inside the shim.
        .library(name: "GameCenter", targets: ["GameCenter"]),
        .library(name: "Review", targets: ["Review"]),
        .library(name: "Notifications", targets: ["Notifications"]),
    ],
    dependencies: [
        .package(
            url: "https://github.com/googleads/swift-package-manager-google-mobile-ads",
            exact: "12.14.0"
        ),
        // Consumers inherit both exact SDK versions; do not override them.
        .package(
            url: "https://github.com/googleads/swift-package-manager-google-user-messaging-platform.git",
            exact: "3.1.0"
        ),
    ],
    targets: [
        .target(
            name: "Platform",
            linkerSettings: [.linkedFramework("UIKit"), .linkedFramework("Foundation")]
        ),
        .target(
            name: "Store",
            dependencies: ["Platform"],
            linkerSettings: [.linkedFramework("StoreKit"), .linkedFramework("UIKit")]
        ),
        .target(
            name: "Ads",
            dependencies: [
                "Att",
                "Platform",
                .product(
                    name: "GoogleMobileAds",
                    package: "swift-package-manager-google-mobile-ads"
                ),
                .product(
                    name: "GoogleUserMessagingPlatform",
                    package: "swift-package-manager-google-user-messaging-platform"
                ),
            ],
            linkerSettings: [.linkedFramework("UIKit")]
        ),
        .target(
            name: "Att",
            dependencies: ["Platform"],
            linkerSettings: [
                .linkedFramework("UIKit"),
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
        .target(
            name: "Notifications",
            dependencies: ["Platform"],
            linkerSettings: [.linkedFramework("UserNotifications")]
        ),
    ]
)
