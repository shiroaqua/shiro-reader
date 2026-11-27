import { useEffect } from "react"
import { AppRuntimeProvider } from "@shiro/ui-shared/context/app-runtime-context"
import type { RustErrorEvent } from "@shiro/ui-shared/lib/error-notifications"
import { useTranslation } from "@shiro/ui-shared/lib/i18n"
import { DesktopReaderChrome } from "./DesktopReaderChrome"

export interface DesktopReaderAppProps {
  readonly windowId: string
  readonly onCloseWindowFallback?: (() => void) | undefined
}

function isReaderDomainRustError(event: RustErrorEvent): boolean {
  return (
    event.key.startsWith("client.reader.") ||
    (event.code >= 3100 && event.code < 3200)
  )
}

export function DesktopReaderApp({
  windowId,
  onCloseWindowFallback,
}: DesktopReaderAppProps) {
  const { locale } = useTranslation()

  useEffect(() => {
    document.documentElement.lang = locale === "zh" ? "zh-CN" : "en"
  }, [locale])

  return (
    <AppRuntimeProvider
      replayBuffered={false}
      listenGlobalErrors={true}
      errorFilter={isReaderDomainRustError}
    >
      <DesktopReaderChrome
        windowId={windowId}
        onCloseWindowFallback={onCloseWindowFallback}
      />
    </AppRuntimeProvider>
  )
}
