#!/usr/bin/env node
const { execFileSync } = require("child_process");
const path = require("path");
const fs = require("fs");

const PLATFORMS = {
  "win32-x64": { pkg: "@ollezetterstrom/rustmix-win32-x64", bin: "rustmix.exe" },
  "linux-x64": { pkg: "@ollezetterstrom/rustmix-linux-x64", bin: "rustmix" },
  "linux-arm64": { pkg: "@ollezetterstrom/rustmix-linux-arm64", bin: "rustmix" },
  "darwin-arm64": { pkg: "@ollezetterstrom/rustmix-darwin-arm64", bin: "rustmix" },
  "darwin-x64": { pkg: "@ollezetterstrom/rustmix-darwin-x64", bin: "rustmix" },
};

const key = `${process.platform}-${process.arch}`;
const target = PLATFORMS[key];

if (!target) {
  console.error(`Unsupported platform/architecture: ${key}`);
  console.error("Rustmix currently supports: win32-x64, linux-x64, linux-arm64, darwin-arm64, darwin-x64");
  process.exit(1);
}

let binPath;
try {
  binPath = require.resolve(`${target.pkg}/bin/${target.bin}`);
} catch (err) {
  // Support local monorepo / relative layouts if present
  const localFallback = path.join(
    __dirname,
    "..",
    "..",
    target.pkg.replace("@ollezetterstrom/", ""),
    "bin",
    target.bin
  );
  if (fs.existsSync(localFallback)) {
    binPath = localFallback;
  } else {
    console.error(`Failed to find Rustmix binary for ${key} (${target.pkg}).`);
    console.error(`Ensure ${target.pkg} is installed or your package manager supports optionalDependencies.`);
    process.exit(1);
  }
}

try {
  execFileSync(binPath, process.argv.slice(2), { stdio: "inherit" });
} catch (err) {
  if (err && err.status !== undefined && err.status !== null) {
    process.exit(err.status);
  }
  process.exit(1);
}
