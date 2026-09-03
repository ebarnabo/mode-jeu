import { spawn } from "node:child_process";
import { readFileSync, existsSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

const keyPath = join(homedir(), ".tauri", "mode-jeu.key");
const passPath = join(homedir(), ".tauri", "mode-jeu.key.pass");

if (!process.env.TAURI_SIGNING_PRIVATE_KEY && existsSync(keyPath)) {
  process.env.TAURI_SIGNING_PRIVATE_KEY = readFileSync(keyPath, "utf8").trim();
}
if (!process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD && existsSync(passPath)) {
  process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD = readFileSync(passPath, "utf8");
}

const child = spawn("npx", ["tauri", "build", ...process.argv.slice(2)], {
  stdio: "inherit",
  env: process.env,
  shell: true,
});

child.on("exit", (code) => process.exit(code ?? 1));
