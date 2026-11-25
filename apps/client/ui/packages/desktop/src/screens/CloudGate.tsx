import { useId, useRef, useState } from "react"
import {
  AlertTriangle,
  Cloud,
  CloudOff,
  HardDrive,
  LoaderCircle,
  Power,
  RefreshCw,
} from "lucide-react"
import { Alert, AlertDescription, AlertTitle } from "@shiro/ui-shared/components/ui/alert"
import { BrandMark } from "@shiro/ui-shared/components/brand-mark"
import { CloudOriginInput } from "@shiro/ui-shared/components/cloud-origin-input"
import { Button } from "@shiro/ui-shared/components/ui/button"
import { Input } from "@shiro/ui-shared/components/ui/input"
import { useAppRuntime } from "@shiro/ui-shared/context/app-runtime-context"
import { useTranslation, type Locale } from "@shiro/ui-shared/lib/i18n"

export function DesktopCloudGateScreen() {
  const { t, locale } = useTranslation()
  const {
    bootstrap,
    changeLocale,
    updateSettings,
    cloudProbe,
    cloudUpdateOrigin,
    cloudLogin,
    exitApp,
  } = useAppRuntime()

  const usernameId = useId()
  const passwordId = useId()
  const originInputId = useId()

  const [username, setUsername] = useState(
    bootstrap?.cloud?.user?.username ?? ""
  )
  const [password, setPassword] = useState("")
  const [editingOrigin, setEditingOrigin] = useState(false)
  const [originDraft, setOriginDraft] = useState(
    bootstrap?.cloud?.origin ?? bootstrap?.cloudOrigin ?? ""
  )
  const [originValid, setOriginValid] = useState(false)
  const [busyAction, setBusyAction] = useState<string | null>(null)
  const loginSubmitting = useRef(false)

  if (!bootstrap || !bootstrap.cloud) return null

  const cloud = bootstrap.cloud
  const phase = bootstrap.phase
  const switchedToLocal = bootstrap.configuredMode === "local"

  const runAction = async (key: string, fn: () => Promise<unknown>) => {
    if (busyAction !== null) return
    setBusyAction(key)
    try {
      await fn()
    } catch {
      // Rust host emits localized shiro://error notification.
    } finally {
      setBusyAction(null)
    }
  }

  const handleLocale = (nextLocale: Locale) =>
    void runAction("locale", () => changeLocale(nextLocale))

  const handleRetry = () => void runAction("probe", () => cloudProbe())

  const handleOriginSubmit = (event: React.FormEvent) => {
    event.preventDefault()
    const trimmed = originDraft.trim()
    if (!trimmed) return
    void runAction("origin", async () => {
      await cloudUpdateOrigin(trimmed)
      setEditingOrigin(false)
    })
  }

  const handleLoginSubmit = async (event: React.FormEvent) => {
    event.preventDefault()
    if (loginSubmitting.current || busyAction !== null) return
    loginSubmitting.current = true
    setBusyAction("login")
    try {
      await cloudLogin(username, password)
      setPassword("")
    } catch {
      // Rust host emits localized error event; preserve username input.
    } finally {
      loginSubmitting.current = false
      setBusyAction(null)
    }
  }

  const handleSwitchLocal = () =>
    void runAction("switch-local", () =>
      updateSettings({
        mode: "local",
        locale: null,
        localLibraryRoot: null,
        cloudOrigin: null,
      })
    )

  const headerIcon =
    phase === "cloud_protocol_incompatible" ? (
      <AlertTriangle className="size-4 text-destructive" />
    ) : phase === "cloud_unreachable" ? (
      <CloudOff className="size-4 text-muted-foreground" />
    ) : (
      <Cloud className="size-4 text-primary" />
    )

  const titleKey =
    phase === "cloud_protocol_incompatible"
      ? "local.cloud.protocol.title"
      : phase === "cloud_unreachable"
        ? "local.cloud.unreachable.title"
        : "local.cloud.login.title"

  const showOriginEditor =
    editingOrigin ||
    phase === "cloud_protocol_incompatible" ||
    phase === "cloud_unreachable"

  return (
    <div className="relative flex min-h-dvh items-center justify-center overflow-hidden bg-background p-6">
      <div className="pointer-events-none absolute inset-0" aria-hidden="true">
        <div className="absolute -top-1/4 -right-1/4 size-[600px] rounded-full bg-primary/5 blur-3xl" />
        <div className="absolute -bottom-1/4 -left-1/4 size-[500px] rounded-full bg-primary/5 blur-3xl" />
      </div>

      <div className="relative z-10 w-full max-w-lg space-y-5 rounded-2xl border bg-card p-6 shadow-xl shadow-black/[0.03]">
        {/* Header + Language switcher */}
        <div className="flex items-start justify-between gap-3">
          <div className="flex items-center gap-3">
            <BrandMark className="size-11 shrink-0 rounded-xl" />
            <div>
              <h1 className="text-lg font-semibold tracking-tight">
                {t(titleKey)}
              </h1>
              <p className="flex items-center gap-1.5 text-xs text-muted-foreground">
                {headerIcon}
                <span>{t("local.mode.cloud")}</span>
                {cloud.protocolVersion !== null && (
                  <span className="rounded bg-muted px-1.5 py-0.5 font-mono text-[10px]">
                    v{cloud.protocolVersion}
                  </span>
                )}
              </p>
            </div>
          </div>
          <div className="flex shrink-0 gap-1">
            <Button
              type="button"
              size="xs"
              variant={locale === "zh" ? "secondary" : "ghost"}
              onClick={() => handleLocale("zh")}
              disabled={busyAction !== null}
            >
              中文
            </Button>
            <Button
              type="button"
              size="xs"
              variant={locale === "en" ? "secondary" : "ghost"}
              onClick={() => handleLocale("en")}
              disabled={busyAction !== null}
            >
              EN
            </Button>
          </div>
        </div>

        {/* Active origin banner */}
        <div className="space-y-1">
          <div className="flex items-center justify-between gap-2">
            <span className="text-xs font-medium text-muted-foreground">
              {t("local.cloud.server_origin")}
            </span>
            {!showOriginEditor && (
              <button
                type="button"
                className="text-xs font-medium text-primary underline-offset-4 hover:underline"
                onClick={() => {
                  setOriginDraft(cloud.origin)
                  setEditingOrigin(true)
                }}
              >
                {t("local.cloud.change_origin")}
              </button>
            )}
          </div>
          <div className="rounded-lg border bg-muted/40 px-3 py-2 font-mono text-xs break-all">
            {cloud.origin}
          </div>
        </div>

        {/* Protocol Incompatible View */}
        {phase === "cloud_protocol_incompatible" && (
          <div className="space-y-3">
            <Alert variant="destructive">
              <AlertTriangle />
              <AlertTitle>{t("local.cloud.protocol.alert_title")}</AlertTitle>
              <AlertDescription className="space-y-1">
                <p>
                  {cloud.diagnostic?.message ??
                    t("local.cloud.protocol.alert_desc")}
                </p>
                {cloud.diagnostic?.detail && (
                  <p className="font-mono text-xs break-all opacity-90">
                    {cloud.diagnostic.detail}
                  </p>
                )}
              </AlertDescription>
            </Alert>
            <Button
              type="button"
              variant="outline"
              onClick={handleRetry}
              disabled={busyAction !== null}
            >
              {busyAction === "probe" ? (
                <LoaderCircle className="animate-spin" />
              ) : (
                <RefreshCw />
              )}
              {t("local.cloud.retry")}
            </Button>
          </div>
        )}

        {/* Unreachable / Transient Offline View */}
        {phase === "cloud_unreachable" && (
          <div className="space-y-3">
            <Alert>
              <CloudOff />
              <AlertTitle>
                {t("local.cloud.unreachable.alert_title")}
              </AlertTitle>
              <AlertDescription className="space-y-1">
                <p>
                  {cloud.diagnostic?.message ??
                    t("local.cloud.unreachable.alert_desc")}
                </p>
                {cloud.hasSavedSession && (
                  <p className="font-medium text-foreground">
                    {t("local.cloud.unreachable.session_preserved").replace(
                      "{username}",
                      cloud.user?.username ?? t("local.common.user")
                    )}
                  </p>
                )}
              </AlertDescription>
            </Alert>

            <Button
              type="button"
              onClick={handleRetry}
              disabled={busyAction !== null}
            >
              {busyAction === "probe" ? (
                <LoaderCircle className="animate-spin" />
              ) : (
                <RefreshCw />
              )}
              {t("local.cloud.retry")}
            </Button>
          </div>
        )}

        {/* Cloud Login View (Tauri IPC only — tokens stay in Rust) */}
        {phase === "cloud_auth_required" && (
          <div className="space-y-4">
            {cloud.diagnostic && (
              <Alert variant="destructive">
                <AlertTriangle />
                <AlertDescription>{cloud.diagnostic.message}</AlertDescription>
              </Alert>
            )}

            <form onSubmit={handleLoginSubmit} className="space-y-4">
              <div className="space-y-1.5">
                <label
                  htmlFor={usernameId}
                  className="block text-sm font-medium"
                >
                  {t("local.login.username")}
                </label>
                <Input
                  id={usernameId}
                  type="text"
                  autoComplete="username"
                  placeholder={t("local.login.username_placeholder")}
                  value={username}
                  onChange={(e) => setUsername(e.target.value)}
                  disabled={busyAction !== null}
                  className="h-10"
                />
              </div>

              <div className="space-y-1.5">
                <label
                  htmlFor={passwordId}
                  className="block text-sm font-medium"
                >
                  {t("local.login.password")}
                </label>
                <Input
                  id={passwordId}
                  type="password"
                  autoComplete="current-password"
                  placeholder={t("local.login.password_placeholder")}
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  disabled={busyAction !== null}
                  className="h-10"
                />
              </div>

              <Button
                type="submit"
                disabled={busyAction !== null || !username.trim() || !password}
                aria-busy={busyAction === "login"}
                className="h-10 w-full text-sm font-semibold"
              >
                {busyAction === "login" && (
                  <LoaderCircle aria-hidden="true" className="animate-spin" />
                )}
                {busyAction === "login"
                  ? t("local.login.loading")
                  : t("local.login.submit")}
              </Button>
            </form>
          </div>
        )}

        {/* Change Cloud Server Origin inline recovery form (stays in Cloud Mode) */}
        {showOriginEditor && (
          <form
            onSubmit={handleOriginSubmit}
            className="space-y-2 border-t pt-4"
          >
            <label
              htmlFor={originInputId}
              className="block text-xs font-medium text-muted-foreground"
            >
              {t("local.cloud.update_origin_label")}
            </label>
            <div className="flex items-start gap-2">
              <CloudOriginInput
                id={originInputId}
                placeholder={t("local.setup.cloud_origin_placeholder")}
                value={originDraft}
                onChange={setOriginDraft}
                onValidityChange={setOriginValid}
                disabled={busyAction !== null}
                className="h-9"
              />
              <Button
                type="submit"
                size="sm"
                variant="secondary"
                className="h-9"
                disabled={
                  busyAction !== null ||
                  !originDraft.trim() ||
                  !originValid ||
                  originDraft.trim() === cloud.origin
                }
              >
                {busyAction === "origin" && (
                  <LoaderCircle className="animate-spin" />
                )}
                {t("local.cloud.update_origin_submit")}
              </Button>
            </div>
          </form>
        )}

        {/* Switch to Local Mode (next launch) + Exit footer */}
        {switchedToLocal && (
          <Alert>
            <AlertDescription>
              {t("local.cloud.switched_local_notice")}
            </AlertDescription>
          </Alert>
        )}

        <div className="flex flex-wrap items-center gap-2 border-t pt-4">
          {!switchedToLocal && (
            <Button
              type="button"
              size="sm"
              variant="outline"
              onClick={handleSwitchLocal}
              disabled={busyAction !== null}
            >
              {busyAction === "switch-local" ? (
                <LoaderCircle className="animate-spin" />
              ) : (
                <HardDrive />
              )}
              {t("local.cloud.switch_local")}
            </Button>
          )}
          <Button
            type="button"
            size="sm"
            variant="ghost"
            className="ml-auto text-muted-foreground"
            onClick={() => void exitApp()}
            disabled={busyAction !== null}
          >
            <Power />
            {t("local.settings.exit_to_restart")}
          </Button>
        </div>
      </div>
    </div>
  )
}
