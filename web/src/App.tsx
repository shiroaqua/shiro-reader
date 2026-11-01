import { DashboardScreen } from "./screens/Dashboard"
import { LoginScreen } from "./screens/Login"
import { AuthProvider, useAuth } from "./context/auth-context"

function AppContent() {
  const { isAuthenticated, isLoading } = useAuth()

  if (isLoading) {
    return (
      <div className="min-h-screen flex items-center justify-center bg-slate-900">
        <div className="text-white text-xl">Loading...</div>
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
