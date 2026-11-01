import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createHash, randomUUID } from "node:crypto";
import { existsSync } from "node:fs";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { delimiter, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

const directory = dirname(fileURLToPath(import.meta.url));
const repository = resolve(directory, "../..");
const windows = process.platform === "win32";
const powershellArgs = ["-NoLogo", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass"];
const results = { passed: 0, failed: 0, skipped: 0 };

async function main() {
  const options = argumentsForSuite();
  const bash = await findShell("bash", options["require-shells"]);
  const powershells = [];
  for (const name of options.suite === "windows" ? ["powershell", "pwsh"] : ["pwsh"]) {
    const shell = await findShell(name, options["require-shells"]);
    if (shell) powershells.push(shell);
  }

  if (bash) {
    await check("Bash arguments and platform", async () => {
      const source = await readFile(join(directory, "bash-arguments.sh"), "utf8");
      await run(bash, ["--noprofile", "--norc", "-c", source], {
        ...environment(), INSTALLER_SOURCE: join(repository, "install.sh"),
      });
    });
  }
  for (const shell of powershells) {
    await check(`${shell}: release metadata and corrupt download`, async () => {
      await fixture(Buffer.from("corrupt executable"), "1.2.3", "x86_64-pc-windows-msvc", async (server) => {
        await isolated(server, async (env) => {
          await run(shell, [...powershellArgs, "-File", join(directory, "powershell-metadata.ps1")], env);
        });
        assert.equal(server.requests.filter((path) => path.startsWith("/asset")).length, 1);
        assert(server.requests.includes("/tags/v1.2.3"));
      });
    });
  }
  if (options.suite === "windows") {
    await windowsSuite(options, powershells, bash);
  }
  console.log(`Installers: ${results.passed} passed, ${results.failed} failed, ${results.skipped} skipped`);
  if (results.failed || !results.passed) process.exitCode = 1;
}

function argumentsForSuite() {
  const { values } = parseArgs({
    options: {
      suite: { type: "string", default: "portable" },
      binary: { type: "string" },
      target: { type: "string" },
      "require-shells": { type: "boolean", default: false },
    },
  });
  assert(["portable", "windows"].includes(values.suite), "--suite must be portable or windows");
  if (values.suite === "windows") {
    assert(windows, "The windows suite requires native Windows");
    assert(values.binary, "The windows suite requires --binary pointing to a built Hanamori executable");
    assert(["x86_64-pc-windows-msvc", "aarch64-pc-windows-msvc"].includes(values.target),
      "The windows suite requires --target x86_64-pc-windows-msvc or aarch64-pc-windows-msvc");
    values.binary = resolve(values.binary);
  }
  return values;
}

async function windowsSuite(options, powershells, bash) {
  const versionOutput = await run(options.binary, ["--version"]);
  const version = /^hanamori (\S+)\s*$/.exec(versionOutput);
  assert(version, "--binary did not report a Hanamori version");
  const bytes = await readFile(options.binary);
  const msys = await findShell("msys2_bash", false);
  const shells = [bash, msys].filter(Boolean);
  await fixture(bytes, version[1], options.target, async (server) => {
    for (const shell of powershells) {
      for (const script of ["powershell.ps1", "powershell-failures.ps1"]) {
        await check(`${shell}: ${script}`, async () => {
          await isolated(server, async (env) => {
            env.INSTALLER_BINARY = options.binary;
            await run(shell, [...powershellArgs, "-File", join(directory, script)], env);
          });
        });
      }
    }
    for (const shell of shells) {
      await check(`${shell}: native Bash installation`, async () => {
        assert(powershells.length, "Bash installation tests require PowerShell for registry isolation");
        await isolated(server, async (env) => {
          const installer = await readFile(join(repository, "install.sh"), "utf8");
          assert(installer.includes("https://hanamori.netlify.app/install.ps1"));
          await writeFile(join(env.INSTALLER_ROOT, "install.sh"),
            installer.replace("https://hanamori.netlify.app/install.ps1", `${server.url}/install.ps1`));
          await writeFile(join(env.INSTALLER_ROOT, "install.ps1"), server.backend);
          env.INSTALLER_BASH = shell;
          const other = shells.find((candidate) => candidate !== shell);
          if (other) {
            // Exercise discovery even when PATH initially prefers the other MSYS runtime.
            const foreignBin = await run(other, ["--noprofile", "--norc", "-c", "/usr/bin/cygpath -aw /usr/bin"]);
            env.PATH = `${foreignBin.trim()}${delimiter}${env.PATH}`;
          }
          const before = server.requests.filter((path) => path === "/install.ps1").length;
          await run(powershells[0], [...powershellArgs, "-File", join(directory, "bash-registry.ps1")], env);
          assert(server.requests.filter((path) => path === "/install.ps1").length > before,
            "Piped Bash installer did not download the PowerShell backend");
          assert(!existsSync(join(env.INSTALLER_ROOT, "bare/hanamori.exe.install.json")));
        });
      });
    }
  });
}

async function fixture(bytes, version, target, action) {
  const source = await readFile(join(repository, "install.ps1"), "utf8");
  const invocation = "Install-Hanamori @PSBoundParameters";
  assert(source.includes(invocation));
  // Only redirect external services and registry writes in the temporary backend.
  const backend = source.replace(invocation,
    `${invocation} -ReleaseApi $env:INSTALLER_API -EnvironmentKey $env:INSTALLER_REGISTRY`);
  const digest = `sha256:${createHash("sha256").update(bytes).digest("hex")}`;
  const requests = [];
  let url;
  const server = createServer((request, response) => {
    const path = request.url;
    requests.push(path);
    if (path === "/install.ps1") {
      response.end(backend);
      return;
    }
    if (path === "/asset/missing") {
      response.writeHead(404);
      response.end();
      return;
    }
    if (path.startsWith("/asset")) {
      response.end(bytes);
      return;
    }
    const release = {
      tag_name: `v${version}`, draft: false, prerelease: false, unknown: 42,
      assets: [{ name: `hanamori-${target}.exe`, browser_download_url: `${url}/asset`,
        size: bytes.length, digest }],
    };
    if (target === "x86_64-pc-windows-msvc") {
      release.assets.push({ ...release.assets[0], name: "hanamori-aarch64-pc-windows-msvc.exe",
        browser_download_url: `${url}/asset/arm64` });
    }
    const asset = release.assets[0];
    if (path.startsWith("/prerelease/")) release.prerelease = true;
    if (path.startsWith("/no-digest/")) asset.digest = null;
    if (path.startsWith("/bad-sha/") || path.endsWith("/v9.9.9")) asset.digest = `sha256:${"0".repeat(64)}`;
    if (path.endsWith("/v9.9.9")) release.tag_name = "v9.9.9";
    if (path.startsWith("/missing/")) asset.browser_download_url = `${url}/asset/missing`;
    if (path.startsWith("/wrong-version/")) release.tag_name = "v99.0.0";
    if (path.startsWith("/bad-size/")) asset.size += 1;
    response.setHeader("Content-Type", "application/json");
    response.end(JSON.stringify(release));
  });
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  url = `http://127.0.0.1:${server.address().port}`;
  try {
    await action({ url, version, backend, requests });
  } finally {
    await new Promise((resolve, reject) => server.close((error) => error ? reject(error) : resolve()));
  }
}

async function isolated(server, action) {
  const temporary = await mkdtemp(join(tmpdir(), "hanamori-installer-"));
  const root = join(temporary, "用户 '& [test]");
  await mkdir(root);
  const env = {
    ...environment(),
    INSTALLER_SOURCE: join(repository, "install.ps1"),
    INSTALLER_API: server.url,
    INSTALLER_VERSION: server.version,
    INSTALLER_ROOT: root,
    INSTALLER_REGISTRY: `Software\\Hanamori-Installer-Test-${randomUUID()}`,
    LOCALAPPDATA: join(temporary, "data"),
    HANAMORI_HOME: join(temporary, "manager"),
  };
  try {
    await action(env);
    assert(!existsSync(join(temporary, "manager/toolchains")), "Installer unexpectedly installed a toolchain");
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
}

function environment() {
  // Keep Windows runtime/socket initialization and executable discovery, not inherited toolchain state.
  const keep = new Set([
    "PATH", "PATHEXT", "SYSTEMROOT", "SYSTEMDRIVE", "WINDIR", "COMSPEC", "OS",
    "TEMP", "TMP", "TMPDIR", "HOME", "USERPROFILE", "APPDATA", "LOCALAPPDATA",
    "PROGRAMFILES", "PROGRAMFILES(X86)", "PROGRAMW6432", "PROGRAMDATA",
  ]);
  return Object.fromEntries(Object.entries(process.env)
    .filter(([key]) => keep.has(key.toUpperCase()))
    .map(([key, value]) => [windows ? key.toUpperCase() : key, value]));
}

async function findShell(name, required) {
  const override = `HANAMORI_TEST_${name.toUpperCase()}`;
  const explicit = process.env[override];
  const candidates = [];
  if (explicit !== undefined) {
    candidates.push(explicit);
  } else if (name === "msys2_bash") {
    candidates.push(join(process.env.SystemDrive || "C:", "msys64/usr/bin/bash.exe"));
  } else {
    const filename = `${name}${windows ? ".exe" : ""}`;
    const path = Object.entries(process.env).find(([key]) => key.toUpperCase() === "PATH")?.[1] || "";
    for (const entry of path.split(delimiter)) {
      candidates.push(join(entry, filename));
      if (windows && name === "bash" && existsSync(join(entry, "git.exe"))) {
        candidates.push(join(entry, "../bin/bash.exe"));
      }
    }
    if (windows && name === "bash") {
      candidates.push(join(process.env.ProgramFiles || "C:\\Program Files", "Git/bin/bash.exe"));
    }
    if (windows && name === "powershell") {
      candidates.push(join(process.env.SystemRoot, "System32/WindowsPowerShell/v1.0/powershell.exe"));
    }
  }
  const powershell = name === "powershell" || name === "pwsh";
  const args = powershell ? [...powershellArgs, "-Command", "exit 0"]
    : windows ? ["--noprofile", "--norc", "-c", "command -v cygpath"] : ["--version"];
  for (const candidate of candidates) {
    if (!existsSync(candidate)) continue;
    try {
      await run(candidate, args);
      return resolve(candidate);
    } catch {
      // A Windows PATH can contain WSL's bash.exe instead of a native MSYS runtime.
    }
  }
  if (required || explicit !== undefined) throw new Error(`Required shell unavailable: ${name}; set ${override}`);
  results.skipped += 1;
  console.log(`SKIP ${name}: unavailable; set ${override}`);
  return undefined;
}

async function check(name, action) {
  try {
    await action();
    results.passed += 1;
    console.log(`PASS ${name}`);
  } catch (error) {
    results.failed += 1;
    console.error(`FAIL ${name}\n${error.stack}`);
  }
}

function run(program, args, env = environment()) {
  return new Promise((resolve, reject) => {
    const child = spawn(program, args, { env, stdio: ["ignore", "pipe", "pipe"] });
    let stdout = "";
    let stderr = "";
    child.stdout.setEncoding("utf8").on("data", (chunk) => { stdout += chunk; });
    child.stderr.setEncoding("utf8").on("data", (chunk) => { stderr += chunk; });
    child.on("error", reject);
    child.on("close", (code, signal) => {
      if (code === 0) resolve(stdout);
      else reject(new Error(`${program} exited with ${code ?? signal}\n${stdout}\n${stderr}`));
    });
  });
}

main().catch((error) => {
  console.error(error.stack);
  process.exitCode = 1;
});
