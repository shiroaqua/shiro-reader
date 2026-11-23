#!/usr/bin/env node

import path from "node:path"
import process from "node:process"
import { fail, handleCliError, parseArgs, printUsage } from "./lib/cli.mjs"
import { cmdBuild } from "./lib/build.mjs"
import { cmdPrepare } from "./lib/prepare.mjs"
import { cmdSmoke } from "./lib/smoke.mjs"
import { verifyArtifact } from "./lib/verify.mjs"

async function main() {
  const { command, flags } = parseArgs(process.argv.slice(2))
  switch (command) {
    case "help":
      printUsage()
      return
    case "prepare":
      await cmdPrepare(flags)
      return
    case "build":
      await cmdBuild(flags)
      return
    case "verify":
      if (!flags.target || !flags.artifact) {
        fail("verify requires --target and --artifact")
      }
      console.log(
        JSON.stringify(verifyArtifact(flags.target, path.resolve(flags.artifact)), null, 2)
      )
      return
    case "smoke":
      await cmdSmoke(flags)
      return
    default:
      fail(`unknown subcommand: ${command}`)
  }
}

try {
  await main()
} catch (err) {
  handleCliError(err)
}
