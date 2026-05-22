/**
 * WebdriverIO config for Tauri WebDriver smoke tests.
 *
 * Spawns `tauri-driver` as a child process, which wraps the platform-native
 * WebDriver server (WebKitGTK on Linux, Edge WebView2 on Windows).
 *
 * Refs: #188, https://tauri.app/develop/tests/webdriver/
 */

const os = require("os");
const path = require("path");
const { spawn } = require("child_process");

// Platform-specific binary name
const isWindows = os.platform() === "win32";
const binaryName = isWindows ? "hyrr-desktop.exe" : "hyrr-desktop";
const binaryPath = path.resolve(
  __dirname,
  "..",
  "src-tauri",
  "target",
  "debug",
  binaryName,
);

// tauri-driver binary — installed via `cargo install tauri-driver`
const tauriDriverBin = path.resolve(
  os.homedir(),
  ".cargo",
  "bin",
  isWindows ? "tauri-driver.exe" : "tauri-driver",
);

let tauriDriver;

exports.config = {
  hostname: "127.0.0.1",
  port: 4444,
  specs: ["./specs/**/*.spec.js"],
  maxInstances: 1,
  capabilities: [
    {
      maxInstances: 1,
      "tauri:options": {
        application: binaryPath,
      },
    },
  ],

  framework: "mocha",
  reporters: ["spec"],
  mochaOpts: {
    ui: "bdd",
    timeout: 60_000,
  },

  // Spawn tauri-driver before each test session
  beforeSession() {
    tauriDriver = spawn(tauriDriverBin, [], {
      stdio: [null, process.stdout, process.stderr],
    });

    // Give tauri-driver a moment to bind the port
    return new Promise((resolve) => setTimeout(resolve, 500));
  },

  // Kill tauri-driver after each test session
  afterSession() {
    if (tauriDriver) {
      tauriDriver.kill();
      tauriDriver = undefined;
    }
  },
};
