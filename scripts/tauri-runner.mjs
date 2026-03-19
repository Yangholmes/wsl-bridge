import { spawn } from "node:child_process";

const rawArgs = process.argv.slice(2);
const isSuperUserBuild = rawArgs.includes("--su");
const args = rawArgs.filter((arg) => arg !== "--su");

if (args.length === 0) {
  console.error("Usage: pnpm tauri <dev|build> [...args]");
  process.exit(1);
}

const [subcommand, ...rest] = args;
const tauriArgs = [subcommand, "-c", "src-tauri/app/tauri.conf.json", ...rest];
const escapedArgs = tauriArgs.map((arg) => JSON.stringify(arg)).join(" ");
const child = spawn(`pnpm exec tauri ${escapedArgs}`, {
  stdio: "inherit",
  shell: true,
  env: {
    ...process.env,
    WSL_BRIDGE_BUILD_FLAVOR: isSuperUserBuild ? "su" : "standard"
  }
});

child.on("exit", (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
    return;
  }
  process.exit(code ?? 1);
});
