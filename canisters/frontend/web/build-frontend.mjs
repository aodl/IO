import { build } from "esbuild";
import { createHash } from "node:crypto";
import { existsSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { cleanGeneratedDirectory } from "./clean-generated.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "../../..");
const publicDir = resolve(here, "../public");
const generatedDir = resolve(publicDir, "generated");
const outTemp = resolve(generatedDir, "app.build.js");
const manifestPath = resolve(generatedDir, "frontend-bundle.json");

function env(...names) {
  for (const name of names) {
    const value = process.env[name];
    if (value && value.trim()) return value.trim();
  }
  return "";
}

function resolveCanisterId(name) {
  const explicit = env(`CANISTER_ID_${name.toUpperCase()}`);
  if (explicit) return explicit;
  for (const file of [
    resolve(root, ".dfx/local/canister_ids.json"),
    resolve(root, "canister_ids.json"),
  ]) {
    if (!existsSync(file)) continue;
    try {
      const json = JSON.parse(readFileSync(file, "utf8"));
      const local = json[name]?.local;
      const ic = json[name]?.ic;
      return process.env.ICP_NETWORK === "ic" ? ic ?? "" : local ?? "";
    } catch {
      return "";
    }
  }
  return "";
}

export async function buildFrontend() {
  const config = {
    network: env("IO_FRONTEND_NETWORK", "ICP_ENVIRONMENT", "ICP_NETWORK"),
    historianCanisterId: env("CANISTER_ID_IO_HISTORIAN") || resolveCanisterId("io_historian"),
    frontendCanisterId: env("CANISTER_ID_FRONTEND") || resolveCanisterId("frontend"),
    streamManagerCanisterId: env("CANISTER_ID_IO_STREAM_MANAGER") || resolveCanisterId("io_stream_manager"),
    ioLedgerCanisterId: env("CANISTER_ID_IO_LEDGER") || resolveCanisterId("io_ledger"),
    assetVersion: "",
    demoMode: env("IO_FRONTEND_DEMO") === "true",
  };

  cleanGeneratedDirectory();

  await build({
    entryPoints: [resolve(here, "src/main.js")],
    bundle: true,
    outfile: outTemp,
    format: "esm",
    platform: "browser",
    target: "es2022",
    sourcemap: false,
    minify: true,
    legalComments: "none",
    define: {
      __IO_FRONTEND_CONFIG__: JSON.stringify(config),
    },
  });

  const bundleBytes = readFileSync(outTemp);
  const hash = createHash("sha256").update(bundleBytes).digest("hex").slice(0, 12);
  const bundleName = `app.${hash}.js`;
  const bundlePath = resolve(generatedDir, bundleName);
  writeFileSync(bundlePath, bundleBytes);
  rmSync(outTemp);

  config.assetVersion = hash;
  const template = readFileSync(resolve(here, "index.template.html"), "utf8");
  const html = template
    .replaceAll("__APP_BUNDLE_PATH__", `/generated/${bundleName}`)
    .replaceAll("__ASSET_VERSION__", hash);
  writeFileSync(resolve(publicDir, "index.html"), html);
  writeFileSync(
    manifestPath,
    `${JSON.stringify({ bundle: `/generated/${bundleName}`, sha256: hash, config }, null, 2)}\n`,
  );

  console.log(`/generated/${bundleName}`);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  await buildFrontend();
}
