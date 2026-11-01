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
    <div className="min-h-screen flex items-center justify-center bg-slate-50 dark:bg-slate-900">
      <div className="w-full max-w-md px-4">
        <div className="bg-white dark:bg-slate-800 rounded-lg shadow-lg p-8">
          <div className="text-center mb-8">
            <h1 className="text-4xl font-bold text-slate-900 dark:text-white mb-2">ShiroReader</h1>
            <p className="text-slate-600 dark:text-slate-400">{t("login.subtitle")}</p>
          </div>

          <form onSubmit={handleSubmit} className="space-y-4">
            <div className="space-y-2">
              <label className="block text-sm font-medium text-slate-700 dark:text-slate-300">
                {t("login.username")}
              </label>
              <Input
                type="text"
                placeholder={t("login.username_placeholder")}
                value={username}
                onChange={(e) => setUsername(e.target.value)}
                disabled={isLoading}
                className="bg-slate-50 dark:bg-slate-700 dark:text-white dark:placeholder:text-slate-500"
              />
            </div>

            <div className="space-y-2">
              <label className="block text-sm font-medium text-slate-700 dark:text-slate-300">
                {t("login.password")}
              </label>
              <Input
                type="password"
                placeholder={t("login.password_placeholder")}
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                disabled={isLoading}
                className="bg-slate-50 dark:bg-slate-700 dark:text-white dark:placeholder:text-slate-500"
              />
            </div>

            <Button
              type="submit"
              disabled={isLoading || !username || !password}
              className="w-full mt-6"
            >
              {isLoading ? t("login.loading") : t("login.submit")}
            </Button>
          </form>

          <div className="mt-6 pt-6 border-t border-slate-200 dark:border-slate-700">
            <p className="text-center text-sm text-slate-600 dark:text-slate-400">
              {t("login.no_account")}
              <button className="ml-1 text-blue-600 dark:text-blue-400 hover:underline">
                {t("login.register")}
              </button>
            </p>
          </div>
        </div>
      </div>
    </div>
  )
}
