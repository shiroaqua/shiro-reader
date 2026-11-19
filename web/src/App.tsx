import { useEffect } from "react"
import { DashboardScreen } from "./screens/Dashboard"
import { LoginScreen } from "./screens/Login"
import { AuthProvider, useAuth } from "./context/auth-context"
import { useTranslation } from "./lib/i18n"

function AppContent() {
  const { isAuthenticated, isLoading } = useAuth()
  const { locale, t } = useTranslation()

  useEffect(() => {
    document.documentElement.lang = locale === "zh" ? "zh-CN" : "en"
  }, [locale])

  if (isLoading) {
    return (
      <div className="flex min-h-screen items-center justify-center bg-background">
        <div className="flex flex-col items-center gap-3">
          <div className="size-8 animate-spin rounded-full border-2 border-primary/20 border-t-primary" />
          <span className="text-sm text-muted-foreground">
            {t("local.common.loading")}
          </span>
        </div>
      </div>
    )
  }

  return isAuthenticated ? <DashboardScreen /> : <LoginScreen />
}

export function App() {
  return (
    <AuthProvider>
      <AppContent />
    </AuthProvider>
  )
}

export default App
