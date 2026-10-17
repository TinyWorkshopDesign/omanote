// swift-tools-version:5.9

import PackageDescription

let package = Package(
  name: "tauri-plugin-pencil",
  platforms: [
    .iOS(.v16)
  ],
  products: [
    .library(
      name: "tauri-plugin-pencil",
      type: .static,
      targets: ["tauri-plugin-pencil"])
  ],
  dependencies: [
    .package(name: "Tauri", path: "../.tauri/tauri-api")
  ],
  targets: [
    .target(
      name: "tauri-plugin-pencil",
      dependencies: [
        .byName(name: "Tauri")
      ],
      path: "Sources")
  ]
)
