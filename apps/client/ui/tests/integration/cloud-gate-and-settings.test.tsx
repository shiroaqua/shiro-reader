import { screen, waitFor } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { CommandError } from "@shiro/ui-shared/api/tauri-bridge"
import { SettingsDialog } from "@shiro/ui-shared/features/settings/settings-dialog"
import { DesktopCloudGateScreen } from "@shiro/desktop-ui/screens/CloudGate"
import { MobileCloudGateScreen } from "@shiro/mobile-ui/screens/CloudGate"
import {
  cloudAuthRequiredState,
  cloudIncompatibleState,
  cloudUnreachableState,
  makeCloudStatus,
  makeCommandErrorPayload,
  readyCloudState,
  readyLocalState,
} from "../support/builders"
import { tauriTestHost } from "../support/mock-tauri"
import { renderWithProviders } from "../support/render"

describe("CloudGate screens and SettingsDialog", () => {
  describe("DesktopCloudGateScreen", () => {
    it("renders protocol incompatible state and retries cloud probe", async () => {
      tauriTestHost.onInvoke("get_bootstrap_state", () =>
        cloudIncompatibleState()
      )
      tauriTestHost.onInvoke("cloud_probe", () => cloudIncompatibleState())

      const { user } = renderWithProviders(<DesktopCloudGateScreen />, {
        withRuntime: true,
      })

      expect(
        await screen.findByRole("heading", {
          name: "Incompatible Server Protocol",
        })
      ).toBeInTheDocument()
      expect(screen.getByText("Supported: v1")).toBeInTheDocument()

      await user.click(screen.getByRole("button", { name: "Retry Connection" }))
      expect(tauriTestHost.invokeCalls("cloud_probe")).toHaveLength(1)
    })

    it("shows preserved session notice when unreachable with saved session", async () => {
      tauriTestHost.onInvoke("get_bootstrap_state", () =>
        cloudUnreachableState({
          cloud: makeCloudStatus({
            origin: "https://books.example.com",
            hasSavedSession: true,
            user: { userId: "u1", username: "alice" },
          }),
        })
      )

      renderWithProviders(<DesktopCloudGateScreen />, { withRuntime: true })

      expect(
        await screen.findByText(/Your saved session for alice is preserved/)
      ).toBeInTheDocument()
    })

    it("gates login, preserves username on error, and allows inline origin update or switching local", async () => {
      tauriTestHost.onInvoke("get_bootstrap_state", () =>
        cloudAuthRequiredState()
      )
      tauriTestHost.onInvoke("cloud_login", () => {
        throw new CommandError(
          makeCommandErrorPayload({ key: "auth.invalid_credentials" })
        )
      })
      tauriTestHost.onInvoke("cloud_check_origin", () => ({
        ok: true,
        message: null,
      }))
      tauriTestHost.onInvoke("cloud_update_origin", (args) =>
        cloudAuthRequiredState({ cloudOrigin: String(args?.origin) })
      )
      tauriTestHost.onInvoke("update_settings", () =>
        cloudAuthRequiredState({ configuredMode: "local", pendingRestart: true })
      )

      const { user } = renderWithProviders(<DesktopCloudGateScreen />, {
        withRuntime: true,
      })

      const signInBtn = await screen.findByRole("button", { name: "Sign In" })
      expect(signInBtn).toBeDisabled()

      const usernameInput = screen.getByLabelText("Username")
      const passwordInput = screen.getByLabelText("Password")
      await user.type(usernameInput, "alice")
      await user.type(passwordInput, "wrong-pass")
      expect(signInBtn).toBeEnabled()

      await user.click(signInBtn)
      expect(tauriTestHost.invokeCalls("cloud_login")).toHaveLength(1)
      expect(usernameInput).toHaveValue("alice")

      // Change server URL inline
      await user.click(screen.getByRole("button", { name: "Change URL" }))
      const originInput = screen.getByLabelText("Update Server URL")
      const connectBtn = screen.getByRole("button", { name: "Connect" })
      expect(connectBtn).toBeDisabled()

      await user.clear(originInput)
      await user.type(originInput, "https://new.example.com")
      await user.tab()

      await waitFor(() => {
        expect(connectBtn).toBeEnabled()
      })
      await user.click(connectBtn)
      expect(
        tauriTestHost.invokeCalls("cloud_update_origin")[0]!.args
      ).toEqual({ origin: "https://new.example.com" })

      // Switch to local mode -> notice displayed and button removed
      await user.click(
        screen.getByRole("button", {
          name: "Switch to Local Mode (Next Launch)",
        })
      )
      expect(
        await screen.findByText(/Local Mode is saved for the next launch/)
      ).toBeInTheDocument()
      expect(
        screen.queryByRole("button", {
          name: "Switch to Local Mode (Next Launch)",
        })
      ).not.toBeInTheDocument()
    })
  })

  describe("MobileCloudGateScreen", () => {
    it("renders protocol incompatible state and retries cloud probe", async () => {
      tauriTestHost.setPlatform("android")
      tauriTestHost.onInvoke("get_bootstrap_state", () =>
        cloudIncompatibleState({ canPickLibraryDir: false })
      )
      tauriTestHost.onInvoke("cloud_probe", () =>
        cloudIncompatibleState({ canPickLibraryDir: false })
      )

      const { user } = renderWithProviders(<MobileCloudGateScreen />, {
        withRuntime: true,
      })

      expect(
        await screen.findByRole("heading", {
          name: "Incompatible Server Protocol",
        })
      ).toBeInTheDocument()
      expect(screen.getByText("Supported: v1")).toBeInTheDocument()

      await user.click(screen.getByRole("button", { name: "Retry Connection" }))
      expect(tauriTestHost.invokeCalls("cloud_probe")).toHaveLength(1)
    })

    it("shows preserved session notice when unreachable with saved session", async () => {
      tauriTestHost.setPlatform("android")
      tauriTestHost.onInvoke("get_bootstrap_state", () =>
        cloudUnreachableState({
          canPickLibraryDir: false,
          cloud: makeCloudStatus({
            origin: "https://books.example.com",
            hasSavedSession: true,
            user: { userId: "u1", username: "alice" },
          }),
        })
      )

      renderWithProviders(<MobileCloudGateScreen />, { withRuntime: true })

      expect(
        await screen.findByText(/Your saved session for alice is preserved/)
      ).toBeInTheDocument()
    })

    it("gates login, updates origin, and switches to local mode", async () => {
      tauriTestHost.setPlatform("android")
      tauriTestHost.onInvoke("get_bootstrap_state", () =>
        cloudAuthRequiredState({ canPickLibraryDir: false })
      )
      tauriTestHost.onInvoke("cloud_login", () => {
        throw new CommandError(
          makeCommandErrorPayload({ key: "auth.invalid_credentials" })
        )
      })
      tauriTestHost.onInvoke("cloud_check_origin", () => ({
        ok: true,
        message: null,
      }))
      tauriTestHost.onInvoke("cloud_update_origin", (args) =>
        cloudAuthRequiredState({
          canPickLibraryDir: false,
          cloudOrigin: String(args?.origin),
        })
      )
      tauriTestHost.onInvoke("update_settings", () =>
        cloudAuthRequiredState({
          canPickLibraryDir: false,
          configuredMode: "local",
          pendingRestart: true,
        })
      )

      const { user } = renderWithProviders(<MobileCloudGateScreen />, {
        withRuntime: true,
      })

      const signInBtn = await screen.findByRole("button", { name: "Sign In" })
      expect(signInBtn).toBeDisabled()

      const usernameInput = screen.getByLabelText("Username")
      const passwordInput = screen.getByLabelText("Password")
      await user.type(usernameInput, "alice")
      await user.type(passwordInput, "wrong-pass")
      await user.click(signInBtn)

      expect(tauriTestHost.invokeCalls("cloud_login")).toHaveLength(1)
      expect(usernameInput).toHaveValue("alice")

      await user.click(screen.getByRole("button", { name: "Change URL" }))
      const originInput = screen.getByLabelText("Update Server URL")
      const connectBtn = screen.getByRole("button", { name: "Connect" })
      await user.clear(originInput)
      await user.type(originInput, "https://mobile-new.example.com")
      await user.tab()

      await waitFor(() => {
        expect(connectBtn).toBeEnabled()
      })
      await user.click(connectBtn)
      expect(
        tauriTestHost.invokeCalls("cloud_update_origin")[0]!.args
      ).toEqual({ origin: "https://mobile-new.example.com" })

      await user.click(
        screen.getByRole("button", {
          name: "Switch to Local Mode (Next Launch)",
        })
      )
      expect(
        await screen.findByText(/Local Mode is saved for the next launch/)
      ).toBeInTheDocument()
    })
  })

  describe("SettingsDialog", () => {
    it("disables save until dirty, toggles local folder vs cloud URL, and shows pending restart banner", async () => {
      tauriTestHost.onInvoke("get_bootstrap_state", () =>
        readyLocalState({
          canPickLibraryDir: true,
          defaultLibraryRoot: "/default/lib",
          configuredLibraryRoot: "/default/lib",
          pendingRestart: true,
        })
      )
      tauriTestHost.onInvoke("pick_library_dir", () => "/picked/lib")
      tauriTestHost.onInvoke("cloud_check_origin", () => ({
        ok: true,
        message: null,
      }))
      tauriTestHost.onInvoke("update_settings", () =>
        readyCloudState({ pendingRestart: true })
      )
      tauriTestHost.onInvoke("exit_app", () => undefined)

      const onOpenChange = vi.fn()
      const { user } = renderWithProviders(
        <SettingsDialog open onOpenChange={onOpenChange} />,
        { withRuntime: true }
      )

      const dialog = await screen.findByRole("dialog")
      expect(dialog).toBeInTheDocument()
      expect(
        screen.getByText("Changes will take effect after restarting the app.")
      ).toBeInTheDocument()

      const saveBtn = screen.getByRole("button", { name: "Save" })
      expect(saveBtn).toBeDisabled()

      // Local directory change marks form dirty; Reset returns to clean
      await user.click(screen.getByRole("button", { name: "Change Folder" }))
      expect(await screen.findByText("/picked/lib")).toBeInTheDocument()
      expect(saveBtn).toBeEnabled()

      await user.click(screen.getByRole("button", { name: "Reset to Default" }))
      expect(saveBtn).toBeDisabled()

      // Switch to Cloud Mode -> hides directory picker, requires valid origin probe
      await user.click(screen.getByRole("button", { name: "Cloud Mode" }))
      expect(
        screen.queryByRole("button", { name: "Change Folder" })
      ).not.toBeInTheDocument()

      const urlInput = screen.getByLabelText("Server URL")
      await user.type(urlInput, "https://cloud.example.com")
      await user.tab()

      await waitFor(() => {
        expect(saveBtn).toBeEnabled()
      })
      await user.click(saveBtn)
      expect(tauriTestHost.invokeCalls("update_settings")[0]!.args).toEqual({
        request: {
          mode: "cloud",
          locale: null,
          localLibraryRoot: null,
          cloudOrigin: "https://cloud.example.com",
        },
      })

      await user.click(screen.getByRole("button", { name: "Exit App" }))
      expect(tauriTestHost.invokeCalls("exit_app")).toHaveLength(1)
    })
  })
})
