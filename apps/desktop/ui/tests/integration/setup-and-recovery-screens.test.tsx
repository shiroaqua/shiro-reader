import { screen, waitFor } from "@testing-library/react"
import { describe, expect, it } from "vitest"
import { SetupScreen } from "@/screens/Setup"
import { StartupErrorScreen } from "@/screens/StartupError"
import {
  makeStartupDiagnostic,
  readyLocalState,
  setupRequiredState,
  startupFailedState,
} from "../support/builders"
import { FakeLibrary } from "../support/fake-library"
import { tauriTestHost } from "../support/mock-tauri"
import { renderWithProviders } from "../support/render"

describe("SetupScreen and StartupErrorScreen", () => {
  describe("SetupScreen", () => {
    it("allows choosing and resetting a custom library folder on desktop local setup", async () => {
      tauriTestHost.onInvoke("get_bootstrap_state", () =>
        setupRequiredState({
          canPickLibraryDir: true,
          defaultLibraryRoot: "/default/library",
          configuredLibraryRoot: "/default/library",
        })
      )
      tauriTestHost.onInvoke("pick_library_dir", () => "/custom/books")
      tauriTestHost.onInvoke("complete_initial_setup", () =>
        readyLocalState({ configuredLibraryRoot: "/custom/books" })
      )

      const { user } = renderWithProviders(<SetupScreen />, {
        withRuntime: true,
      })

      expect(await screen.findByText("/default/library")).toBeInTheDocument()

      await user.click(screen.getByRole("button", { name: "Choose Folder" }))
      expect(await screen.findByText("/custom/books")).toBeInTheDocument()

      // Reset back to default, then pick again and submit
      await user.click(screen.getByRole("button", { name: "Use Default" }))
      expect(screen.getByText("/default/library")).toBeInTheDocument()

      await user.click(screen.getByRole("button", { name: "Choose Folder" }))
      await user.click(screen.getByRole("button", { name: "Save & Start" }))

      await waitFor(() => {
        expect(tauriTestHost.invokeCalls("complete_initial_setup")).toHaveLength(
          1
        )
      })
      expect(
        tauriTestHost.invokeCalls("complete_initial_setup")[0].args
      ).toEqual({
        request: {
          mode: "local",
          locale: "en",
          localLibraryRoot: "/custom/books",
          cloudOrigin: null,
        },
      })
    })

    it("switches the color theme through the Rust settings store", async () => {
      tauriTestHost.onInvoke("get_bootstrap_state", () => setupRequiredState())
      tauriTestHost.onInvoke("set_app_theme", (args) =>
        setupRequiredState({ theme: args?.theme as "mint" })
      )

      const { user } = renderWithProviders(<SetupScreen />, {
        withRuntime: true,
      })

      const defaultTheme = await screen.findByRole("radio", { name: "Gray" })
      await waitFor(() => {
        expect(document.documentElement.dataset.theme).toBe("default")
      })
      expect(defaultTheme).toHaveAttribute("aria-checked", "true")

      await user.click(screen.getByRole("radio", { name: "Green" }))

      await waitFor(() => {
        expect(document.documentElement.dataset.theme).toBe("mint")
      })
      expect(tauriTestHost.invokeCalls("set_app_theme")[0].args).toEqual({
        theme: "mint",
      })
      expect(screen.getByRole("radio", { name: "Green" })).toHaveAttribute(
        "aria-checked",
        "true"
      )
      expect(defaultTheme).toHaveAttribute("aria-checked", "false")
    })

    it("hides folder picker and displays private storage hint when platform cannot pick directories", async () => {
      tauriTestHost.onInvoke("get_bootstrap_state", () =>
        setupRequiredState({ canPickLibraryDir: false })
      )

      renderWithProviders(<SetupScreen />, { withRuntime: true })

      expect(
        await screen.findByText(/Android always uses the app's private data/)
      ).toBeInTheDocument()
      expect(
        screen.queryByRole("button", { name: "Choose Folder" })
      ).not.toBeInTheDocument()
    })

    it("gates cloud setup submission on origin connectivity check", async () => {
      tauriTestHost.onInvoke("get_bootstrap_state", () => setupRequiredState())
      tauriTestHost.onInvoke("cloud_check_origin", (args) =>
        args?.origin === "https://books.example.com"
          ? { ok: true, message: null }
          : { ok: false, message: "Unreachable server" }
      )
      tauriTestHost.onInvoke("complete_initial_setup", () => readyLocalState())

      const { user } = renderWithProviders(<SetupScreen />, {
        withRuntime: true,
      })

      await user.click(
        await screen.findByRole("button", { name: /Cloud Mode/ })
      )
      const submitBtn = screen.getByRole("button", { name: "Save & Start" })
      expect(submitBtn).toBeDisabled()

      const urlInput = screen.getByLabelText("Server URL")
      await user.type(urlInput, "http://bad.local")
      await user.tab()

      expect(await screen.findByRole("alert")).toHaveTextContent(
        "Unreachable server"
      )
      expect(submitBtn).toBeDisabled()

      await user.clear(urlInput)
      await user.type(urlInput, "https://books.example.com")
      await user.tab()

      expect(
        await screen.findByLabelText("Server connected")
      ).toBeInTheDocument()
      expect(submitBtn).toBeEnabled()

      await user.click(submitBtn)
      await waitFor(() => {
        expect(tauriTestHost.invokeCalls("complete_initial_setup")).toHaveLength(
          1
        )
      })
      expect(
        tauriTestHost.invokeCalls("complete_initial_setup")[0].args
      ).toEqual({
        request: {
          mode: "cloud",
          locale: "en",
          localLibraryRoot: null,
          cloudOrigin: "https://books.example.com",
        },
      })
    })
  })

  describe("StartupErrorScreen", () => {
    it("displays diagnostics and repairs local directory or retries startup", async () => {
      const diag = makeStartupDiagnostic({
        code: 5102,
        key: "client.local.db_corrupted",
        message: "Corrupted SQLite file",
        path: "/broken/library.db",
        detail: "header mismatch",
      })
      tauriTestHost.onInvoke("get_bootstrap_state", () =>
        startupFailedState({ startupError: diag, canPickLibraryDir: true })
      )
      tauriTestHost.onInvoke("pick_library_dir", () => "/repaired/library")
      tauriTestHost.onInvoke("repair_local_library_root", () =>
        readyLocalState()
      )
      tauriTestHost.onInvoke("retry_startup", () =>
        startupFailedState({ startupError: diag })
      )
      tauriTestHost.onInvoke("exit_app", () => undefined)
      tauriTestHost.attachLibrary(new FakeLibrary())

      const { user } = renderWithProviders(<StartupErrorScreen />, {
        withRuntime: true,
      })

      expect(await screen.findByText("Error Code: 5102")).toBeInTheDocument()
      expect(screen.getByText("Corrupted SQLite file")).toBeInTheDocument()
      expect(screen.getByText("/broken/library.db")).toBeInTheDocument()
      expect(screen.getByText("header mismatch")).toBeInTheDocument()
      expect(
        screen.queryByRole("button", { name: "Reconfigure (Local Mode)" })
      ).not.toBeInTheDocument()

      await user.click(
        screen.getByRole("button", { name: "Choose Another Folder" })
      )
      await waitFor(() => {
        expect(
          tauriTestHost.invokeCalls("repair_local_library_root")[0]?.args
        ).toEqual({ path: "/repaired/library" })
      })

      await user.click(screen.getByRole("button", { name: "Retry Opening" }))
      expect(tauriTestHost.invokeCalls("retry_startup")).toHaveLength(1)

      await user.click(screen.getByRole("button", { name: "Exit App" }))
      expect(tauriTestHost.invokeCalls("exit_app")).toHaveLength(1)
    })

    it("offers local reconfiguration and locale switching for invalid config diagnostics", async () => {
      tauriTestHost.onInvoke("get_bootstrap_state", () =>
        startupFailedState({
          canPickLibraryDir: false,
          startupError: makeStartupDiagnostic({
            key: "client.config.invalid",
            message: "Malformed settings.json",
          }),
        })
      )
      tauriTestHost.onInvoke("complete_initial_setup", () => readyLocalState())
      tauriTestHost.onInvoke("set_app_locale", (args) =>
        startupFailedState({
          locale: args?.locale as "zh" | "en",
          startupError: makeStartupDiagnostic({ key: "client.config.invalid" }),
        })
      )

      const { user } = renderWithProviders(<StartupErrorScreen />, {
        withRuntime: true,
      })

      const reconfigBtn = await screen.findByRole("button", {
        name: "Reconfigure (Local Mode)",
      })
      expect(
        screen.queryByRole("button", { name: "Choose Another Folder" })
      ).not.toBeInTheDocument()

      await user.click(reconfigBtn)
      expect(
        tauriTestHost.invokeCalls("complete_initial_setup")[0].args
      ).toEqual({
        request: {
          mode: "local",
          locale: "en",
          localLibraryRoot: null,
          cloudOrigin: null,
        },
      })

      await user.click(screen.getByRole("button", { name: "中文" }))
      expect(tauriTestHost.invokeCalls("set_app_locale")[0].args).toEqual({
        locale: "zh",
      })
    })
  })
})
