import { useRef, useState } from "react"
import { useAuth } from "@/context/auth-context"
import { ApiError } from "@/api/http"
import { notifyError } from "@/lib/error-notifications"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { useTranslation } from "@/lib/i18n"

export function LoginScreen() {
  const { t } = useTranslation()
  const { login } = useAuth()
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
      notifyError(error instanceof ApiError ? error.message : "common.error.unknown")
    } finally {
      submitting.current = false
      setIsLoading(false)
    }
  }

  return (
    <div className="relative flex min-h-screen items-center justify-center overflow-hidden bg-background">
      {/* Decorative background */}
      <div className="pointer-events-none absolute inset-0" aria-hidden="true">
        <div className="absolute -top-1/4 -right-1/4 h-[600px] w-[600px] rounded-full bg-primary/5 blur-3xl" />
        <div className="absolute -bottom-1/4 -left-1/4 h-[500px] w-[500px] rounded-full bg-primary/5 blur-3xl" />
      </div>

      <div className="relative z-10 w-full max-w-sm px-4">
        {/* Logo area */}
        <div className="mb-8 text-center">
          <div className="mx-auto mb-4 flex size-16 items-center justify-center rounded-2xl bg-primary/10 shadow-sm">
            <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" className="text-primary">
              <path d="M4 19.5v-15A2.5 2.5 0 0 1 6.5 2H19a1 1 0 0 1 1 1v18a1 1 0 0 1-1 1H6.5a1 1 0 0 1 0-5H20" />
            </svg>
          </div>
          <h1 className="text-3xl font-bold tracking-tight text-foreground">ShiroReader</h1>
          <p className="mt-1.5 text-sm text-muted-foreground">{t("login.subtitle")}</p>
        </div>

        {/* Card */}
        <div className="rounded-2xl border border-border/60 bg-card p-6 shadow-xl shadow-black/[0.03] dark:shadow-black/20">
          <form onSubmit={handleSubmit} className="space-y-5">
            <div className="space-y-1.5">
              <label className="block text-sm font-medium text-foreground">
                {t("login.username")}
              </label>
              <Input
                type="text"
                placeholder={t("login.username_placeholder")}
                value={username}
                onChange={(e) => setUsername(e.target.value)}
                disabled={isLoading}
                className="h-10"
              />
            </div>

            <div className="space-y-1.5">
              <label className="block text-sm font-medium text-foreground">
                {t("login.password")}
              </label>
              <Input
                type="password"
                placeholder={t("login.password_placeholder")}
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                disabled={isLoading}
                className="h-10"
              />
            </div>

            <Button
              type="submit"
              disabled={isLoading || !username || !password}
              className="h-10 w-full text-sm font-semibold"
            >
              {isLoading ? t("login.loading") : t("login.submit")}
            </Button>
          </form>

          <div className="mt-6 border-t border-border/60 pt-5">
            <p className="text-center text-sm text-muted-foreground">
              {t("login.no_account")}
              <button className="ml-1 font-medium text-primary hover:underline">
                {t("login.register")}
              </button>
            </p>
          </div>
        </div>
      </div>
    </div>
  )
}
