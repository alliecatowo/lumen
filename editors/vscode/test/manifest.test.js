// Guards the manifest invariants the extension code relies on.
const test = require("node:test");
const assert = require("node:assert");
const pkg = require("../package.json");

const props = pkg.contributes.configuration.properties;

test("binary path settings have no default, so the bundled server is reachable", () => {
  for (const key of ["lumen.lspPath", "lumen.executablePath", "lumen.binPath"]) {
    assert.ok(props[key], `${key} is declared`);
    assert.strictEqual(props[key].default, undefined, `${key} must not default to a PATH lookup`);
  }
});

test("binary path settings are machine scoped and restricted in untrusted workspaces", () => {
  for (const key of ["lumen.lspPath", "lumen.executablePath", "lumen.binPath"]) {
    assert.strictEqual(props[key].scope, "machine", `${key} scope`);
  }
  const caps = pkg.capabilities.untrustedWorkspaces;
  assert.strictEqual(caps.supported, "limited");
  for (const key of ["lumen.lspPath", "lumen.executablePath", "lumen.binPath"]) {
    assert.ok(caps.restrictedConfigurations.includes(key), `${key} restricted`);
  }
});

test("engines.vscode satisfies vscode-languageclient 10 (>= 1.91)", () => {
  const min = pkg.engines.vscode.replace(/^[^\d]*/, "").split(".").map(Number);
  assert.ok(min[0] > 1 || (min[0] === 1 && min[1] >= 91), pkg.engines.vscode);
  assert.ok(pkg.devDependencies["@types/vscode"].includes("1.91"), "types match engines");
});
