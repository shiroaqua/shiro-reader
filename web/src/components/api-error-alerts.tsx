import { useLayoutEffect, useRef, useSyncExternalStore } from "react"
import { AlertCircle, X } from "lucide-react"
import { Alert, AlertAction, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { dismissError, getErrorNotifications, subscribeToErrors } from "@/lib/error-notifications"
import { useTranslation } from "@/lib/i18n"

export function ApiErrorAlerts() {
  const notifications = useSyncExternalStore(subscribeToErrors, getErrorNotifications)
  const { locale, t } = useTranslation()
  const timeFormatter = new Intl.DateTimeFormat(locale, {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hourCycle: "h23",
  })
  const containerRef = useRef<HTMLDivElement>(null)

  useLayoutEffect(() => {
    const container = containerRef.current
    if (!container) return

    const removeOverflow = () => {
      let overflow = container.scrollHeight - container.clientHeight
      const gap = parseFloat(getComputedStyle(container).rowGap) || 0

      // Measure actual alert heights so wrapped translations also fit the viewport.
      for (let index = 0; index < notifications.length && overflow > 1; index++) {
        const alert = container.children[notifications.length - 1 - index]
        if (!alert) break
        overflow -= alert.getBoundingClientRect().height + gap
        dismissError(notifications[index].id)
      }
    }

    removeOverflow()
    const observer = new ResizeObserver(removeOverflow)
    observer.observe(container)
    for (const alert of container.children) observer.observe(alert)
    return () => observer.disconnect()
  }, [notifications])

  return (
    <div ref={containerRef} aria-live="assertive" aria-relevant="additions" className="fixed right-4 bottom-4 z-[100] flex max-h-[calc(100dvh-2rem)] w-max max-w-[calc(100vw-2rem)] flex-col gap-3 overflow-hidden">
      {notifications.toReversed().map((notification) => (
        <Alert key={notification.id} variant="destructive" className="shrink-0 pb-7 shadow-lg" style={{ paddingRight: "0.625rem" }}>
          <AlertCircle />
          <AlertTitle className="pr-8">{t("common.error")}</AlertTitle>
          <AlertDescription
            className="min-w-0"
            style={{ textWrap: "wrap", overflowWrap: "anywhere" }}
          >
            {t(notification.message)}
          </AlertDescription>
          <AlertAction>
            <Button
              type="button"
              variant="ghost"
              size="icon-xs"
              aria-label={t("common.close")}
              onClick={() => dismissError(notification.id)}
            >
              <X />
            </Button>
          </AlertAction>
          <time
            dateTime={new Date(notification.createdAt).toISOString()}
            className="absolute right-2.5 bottom-2 text-xs text-muted-foreground tabular-nums"
          >
            {timeFormatter.format(notification.createdAt)}
          </time>
        </Alert>
      ))}
    </div>
  )
}
