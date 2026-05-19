// swift-tools-version: 6.1
// The swift-tools-version declares the minimum version of Swift required to build this package.

import PackageDescription

// Stage 1 keeps the converter dependency trait-free so the Windows server can
// build without requiring a local llama system library. Zenzai linking is a
// separate packaging step because the Windows fork ships selectable backends.
let kanaKanjiConverterTraits: Set<Package.Dependency.Trait> = []

let package = Package(
    name: "azookey-server",
    products: [
        // Products define the executables and libraries a package produces, making them visible to other packages.
        .library(
            name: "azookey-server",
            type: .dynamic,
            targets: ["azookey-server"]
        ),
        .library(name: "ffi", targets: ["azookey-server"])
    ],
    dependencies: [
        // Dependencies declare other packages that this package depends on.
        // .package(url: /* package url */, from: "1.0.0"),
        .package(
            url: "https://github.com/azookey/AzooKeyKanaKanjiConverter",
            revision: "bbef9d2d99a2e9e69ac3f7e2e07b08474de59a81",
            traits: kanaKanjiConverterTraits
        )
    ],
    targets: [
        // Targets are the basic building blocks of a package, defining a module or a test suite.
        // Targets can depend on other targets in this package and products from dependencies.
        .target(name: "ffi"),
        .target(
            name: "azookey-server",
            dependencies: [
                .product(name: "KanaKanjiConverterModule", package: "AzooKeyKanaKanjiConverter"),
                "ffi"
            ]
        ),
        .testTarget(
            name: "azookey-serverTests",
            dependencies: ["azookey-server"]
        ),
    ]
)
