import type { ReactElement } from "react"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { render, type RenderOptions } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { DesktopApp } from "@shiro/desktop-ui/DesktopApp"
import { MobileApp } from "@shiro/mobile-ui/MobileApp"
import type { BootstrapState } from "@shiro/ui-shared/api/tauri-bridge"
import { ApiErrorAlerts } from "@shiro/ui-shared/components/api-error-alerts"
import { AppRuntimeProvider } from "@shiro/ui-shared/context/app-runtime-context"
import { setAppLocaleOverride, type Locale } from "@shiro/ui-shared/lib/i18n"
import { readyLocalState } from "./builders"
import type { FakeLibrary } from "./fake-library"
import { tauriTestHost } from "./mock-tauri"

export function createTestQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: {
        retry: false,
        gcTime: 0,
      },
      mutations: {
        retry: false,
      },
    },
  })
}

export interface ProviderRenderOptions extends Omit<RenderOptions, "wrapper"> {
  locale?: Locale
  queryClient?: QueryClient
  withRuntime?: boolean
  withErrorAlerts?: boolean
  userEventOptions?: Parameters<typeof userEvent.setup>[0]
}

export function renderWithProviders(
  ui: ReactElement,
  options: ProviderRenderOptions = {}
) {
  const {
    locale = "en",
    queryClient = createTestQueryClient(),
    withRuntime = false,
    withErrorAlerts = false,
    userEventOptions,
    ...renderOptions
  } = options

  setAppLocaleOverride(locale)
  const user = userEvent.setup(userEventOptions)

  const content = withRuntime ? (
    <AppRuntimeProvider>{ui}</AppRuntimeProvider>
  ) : (
    ui
  )

  const result = render(
    <QueryClientProvider client={queryClient}>
      {content}
      {withErrorAlerts && <ApiErrorAlerts />}
    </QueryClientProvider>,
    renderOptions
  )

  return {
    user,
    queryClient,
    ...result,
  }
}

export interface AppRenderOptions {
  preview?: boolean
  isTauri?: boolean
  bootstrap?: BootstrapState | (() => Promise<BootstrapState>)
  library?: FakeLibrary
  locale?: Locale
  queryClient?: QueryClient
  userEventOptions?: Parameters<typeof userEvent.setup>[0]
}

function renderTargetApp(
  element: ReactElement,
  options: Omit<AppRenderOptions, "preview"> = {}
) {
  const {
    isTauri = true,
    bootstrap = readyLocalState(),
    library,
    locale = "en",
    queryClient = createTestQueryClient(),
    userEventOptions,
  } = options

  tauriTestHost.setTauriRuntime(isTauri)
  tauriTestHost.onInvoke("get_bootstrap_state", () =>
    typeof bootstrap === "function" ? bootstrap() : bootstrap
  )
  if (library) {
    tauriTestHost.attachLibrary(library)
  }

  setAppLocaleOverride(locale)
  const user = userEvent.setup(userEventOptions)

  const result = render(
    <QueryClientProvider client={queryClient}>{element}</QueryClientProvider>
  )

  return {
    user,
    queryClient,
    ...result,
  }
}

export function renderDesktopApp(options: AppRenderOptions = {}) {
  const { preview = false, ...rest } = options
  return renderTargetApp(<DesktopApp preview={preview} />, rest)
}

export function renderMobileApp(options: AppRenderOptions = {}) {
  const { preview = false, ...rest } = options
  return renderTargetApp(<MobileApp preview={preview} />, rest)
}
