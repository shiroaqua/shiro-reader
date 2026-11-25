import { useLayoutEffect, useMemo, useRef, useSyncExternalStore } from "react"
import { AlertCircle, X } from "lucide-react"
import { Alert, AlertAction, AlertDescription, AlertTitle } from "@shiro/ui-shared/components/ui/alert"
import { Button } from "@shiro/ui-shared/components/ui/button"
import {
  dismissError,
  dismissErrors,
  getErrorNotifications,
  subscribeToErrors,
} from "@shiro/ui-shared/lib/error-notifications"
import { translateMessage, useTranslation } from "@shiro/ui-shared/lib/i18n"

export function ApiErrorAlerts() {
  const notifications = useSyncExternalStore(
    subscribeToErrors,
    getErrorNotifications
  )
  const { locale, t } = useTranslation()
  const timeFormatter = useMemo(
    () =>
      new Intl.DateTimeFormat(locale, {
        hour: "2-digit",
        minute: "2-digit",
        second: "2-digit",
        hourCycle: "h23",
      }),
    [locale]
  )
  const containerRef = useRef<HTMLDivElement>(null)

  useLayoutEffect(() => {
    const container = containerRef.current
    if (!container || notifications.length === 0) return

    const removeOverflow = () => {
      let overflow = container.scrollHeight - container.clientHeight
      const gap = parseFloat(getComputedStyle(container).rowGap) || 0
      const overflowIds: number[] = []

      // Measure actual alert heights so wrapped translations also fit the remaining dock space.
      for (let index = 0; index < notifications.length && overflow > 1; index++) {
        const alert = container.children[notifications.length - 1 - index]
        if (!alert) break
        overflow -= alert.getBoundingClientRect().height + gap
        overflowIds.push(notifications[index]!.id)
      }
      if (overflowIds.length > 0) {
        dismissErrors(overflowIds)
      }
    }

    removeOverflow()
    const observer = new ResizeObserver(removeOverflow)
    observer.observe(container)
    for (const alert of container.children) observer.observe(alert)
    return () => observer.disconnect()
  }, [notifications])

  if (notifications.length === 0) return null

  return (
    <div
      ref={containerRef}
      data-error-alerts=""
      aria-live="assertive"
      aria-relevant="additions"
      className="pointer-events-auto flex min-h-0 w-max max-w-full flex-1 flex-col gap-3 overflow-hidden"
    >
      {notifications.toReversed().map((notification) => (
        <Alert
          key={notification.id}
          variant="destructive"
          className="shrink-0 pb-7 shadow-lg"
          style={{ paddingRight: "0.625rem" }}
        >
          <AlertCircle />
          <AlertTitle className="pr-8">{t("local.common.error")}</AlertTitle>
          <AlertDescription
            className="min-w-0"
            style={{ textWrap: "wrap", overflowWrap: "anywhere" }}
          >
            {translateMessage(notification.translation, locale)}
          </AlertDescription>
          <AlertAction>
            <Button
              type="button"
              variant="ghost"
              size="icon-xs"
              aria-label={t("local.common.close")}
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
