import { useId, useRef, useState } from "react"
import { LoaderCircle } from "lucide-react"
import { useAuth } from "@/context/auth-context"
import { ApiError } from "@/api/http"
import { notifyError } from "@/lib/error-notifications"
import { BrandMark } from "@/components/brand-mark"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { useTranslation } from "@/lib/i18n"

export function LoginScreen() {
  const { t } = useTranslation()
  const { login } = useAuth()
  const usernameId = useId()
  const passwordId = useId()
  const submitting = useRef(false)
  const [username, setUsername] = useState("")
  const [password, setPassword] = useState("")
  const [isLoading, setIsLoading] = useState(false)

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (submitting.current) return
    submitting.current = true
    setIsLoading(true)
    try {
      await login(username, password)
    } catch (error) {
      notifyError(
        error instanceof ApiError
          ? error.translation
          : { source: "local", key: "local.common.error.unknown" }
      )
    } finally {
      submitting.current = false
      setIsLoading(false)
    }
  }

  return (
    <div className="relative flex min-h-screen items-center justify-center overflow-hidden bg-background">
      {/* Decorative background */}
      <div className="pointer-events-none absolute inset-0" aria-hidden="true">
        <div className="absolute -top-1/4 -right-1/4 size-[600px] rounded-full bg-primary/5 blur-3xl" />
        <div className="absolute -bottom-1/4 -left-1/4 size-[500px] rounded-full bg-primary/5 blur-3xl" />
      </div>

      <div className="relative z-10 w-full max-w-sm px-4">
        {/* Logo area */}
        <div className="mb-8 flex flex-col items-center text-center">
          <BrandMark className="mb-4 size-16 rounded-2xl [&_svg:not([class*='size-'])]:size-8" />
          <h1 className="text-3xl font-bold tracking-tight">ShiroReader</h1>
          <p className="mt-1.5 text-sm text-muted-foreground">
            {t("local.login.subtitle")}
          </p>
        </div>

        {/* Card */}
        <div className="rounded-2xl border bg-card p-6 shadow-xl shadow-black/[0.03] dark:shadow-black/20">
          <form onSubmit={handleSubmit} className="space-y-5">
            <div className="space-y-1.5">
              <label htmlFor={usernameId} className="block text-sm font-medium">
                {t("local.login.username")}
              </label>
              <Input
                id={usernameId}
                type="text"
                autoComplete="username"
                placeholder={t("local.login.username_placeholder")}
                value={username}
                onChange={(e) => setUsername(e.target.value)}
                disabled={isLoading}
                className="h-10"
              />
            </div>

            <div className="space-y-1.5">
              <label htmlFor={passwordId} className="block text-sm font-medium">
                {t("local.login.password")}
              </label>
              <Input
                id={passwordId}
                type="password"
                autoComplete="current-password"
                placeholder={t("local.login.password_placeholder")}
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                disabled={isLoading}
                className="h-10"
              />
            </div>

            <Button
              type="submit"
              disabled={isLoading || !username || !password}
              aria-busy={isLoading}
              className="h-10 w-full text-sm font-semibold"
            >
              {isLoading && (
                <LoaderCircle aria-hidden="true" className="animate-spin" />
              )}
              {isLoading ? t("local.login.loading") : t("local.login.submit")}
            </Button>
          </form>

          <div className="mt-6 border-t pt-5">
            <p className="text-center text-sm text-muted-foreground">
              {t("local.login.no_account")}
              <button
                type="button"
                className="ml-1 font-medium text-primary underline-offset-4 hover:underline"
              >
                {t("local.login.register")}
              </button>
            </p>
          </div>
        </div>
      </div>
    </div>
  )
}
