import process from "node:process"

export class PackagingError extends Error {
  constructor(message) {
    super(message)
    this.name = "PackagingError"
  }
}

export function log(message) {
  console.log(`[packaging] ${message}`)
}

export function fail(message) {
  throw new PackagingError(message)
}

const COMMAND_SPECS = {
  prepare: {
    booleans: new Set(["offline"]),
    values: new Set(["target"]),
  },
  build: {
    booleans: new Set(["offline", "debug"]),
    values: new Set(["target", "format"]),
  },
  verify: {
    booleans: new Set([]),
    values: new Set(["target", "artifact"]),
  },
  smoke: {
    booleans: new Set([]),
    values: new Set(["target", "artifact", "device"]),
  },
}

export function parseArgs(argv) {
  const [command, ...rest] = argv
  if (!command || command === "-h" || command === "--help") {
    return { command: "help", flags: {} }
  }

  const spec = COMMAND_SPECS[command]
  if (!spec) {
    fail(`unknown subcommand: ${command}`)
  }

  const flags = {}
  for (let i = 0; i < rest.length; i += 1) {
    const token = rest[i]
    if (!token.startsWith("--")) {
      fail(`unexpected positional argument: ${token}`)
    }

    const eqIndex = token.indexOf("=")
    const key = eqIndex === -1 ? token.slice(2) : token.slice(2, eqIndex)
    const inlineValue = eqIndex === -1 ? undefined : token.slice(eqIndex + 1)

    if (spec.booleans.has(key)) {
      if (inlineValue !== undefined) {
        if (inlineValue === "true") {
          flags[key] = true
        } else if (inlineValue === "false") {
          flags[key] = false
        } else {
          fail(`invalid boolean value for --${key}: ${inlineValue}`)
        }
      } else {
        flags[key] = true
      }
      continue
    }

    if (spec.values.has(key)) {
      const value = inlineValue !== undefined ? inlineValue : rest[i + 1]
      if (inlineValue === undefined) {
        i += 1
      }
      if (!value || (inlineValue === undefined && value.startsWith("--"))) {
        fail(`missing value for --${key}`)
      }
      flags[key] = value
      continue
    }

    fail(`unknown flag --${key} for subcommand '${command}'`)
  }

  return { command, flags }
}

export function printUsage() {
  console.log(`Usage:
  node scripts/packaging/package.mjs prepare --target windows-x64|android-arm64|linux-x64 [--offline]
  node scripts/packaging/package.mjs build --target windows-x64 [--offline] [--debug]
  node scripts/packaging/package.mjs build --target android-arm64 [--format apk|aab] [--offline] [--debug]
  node scripts/packaging/package.mjs build --target linux-x64 [--format deb|rpm|appimage] [--offline] [--debug]
  node scripts/packaging/package.mjs verify --target windows-x64|android-arm64|linux-x64 --artifact <path>
  node scripts/packaging/package.mjs smoke --target windows-x64|android-arm64|linux-x64 --artifact <path> [--device <id>]`)
}

export function handleCliError(err) {
  if (err instanceof PackagingError) {
    console.error(`[packaging] ERROR: ${err.message}`)
    process.exit(1)
  }
  console.error(err)
  process.exit(1)
}
