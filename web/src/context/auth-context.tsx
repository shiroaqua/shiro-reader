import { createContext, useContext, useEffect, useSyncExternalStore, type ReactNode } from "react"
import { useQueryClient } from "@tanstack/react-query"
import { getAuthState, subscribeToAuth, restoreSession, monitorSession, login, logout } from "@/api/auth"

interface AuthContextType {
  isAuthenticated: boolean
  username: string | null
  login: (username: string, password: string) => Promise<void>
  logout: () => Promise<void>
  isLoading: boolean
}

const AuthContext = createContext<AuthContextType | undefined>(undefined)

export function AuthProvider({ children }: { children: ReactNode }) {
  const { session, isLoading } = useSyncExternalStore(subscribeToAuth, getAuthState)
  const queryClient = useQueryClient()

  useEffect(() => {
    let previous = getAuthState().session
    const unsubscribe = subscribeToAuth(() => {
      const current = getAuthState().session
      if (previous?.refresh_token !== current?.refresh_token) queryClient.clear()
      previous = current
    })
    void restoreSession()
    const stopMonitoring = monitorSession()
    return () => {
      unsubscribe()
      stopMonitoring()
    }
  }, [queryClient])

  return (
    <AuthContext.Provider value={{ isAuthenticated: session !== null, username: session?.username ?? null, login, logout, isLoading }}>
      {children}
    </AuthContext.Provider>
  )
}

export function useAuth() {
  const context = useContext(AuthContext)
  if (!context) throw new Error("useAuth must be used within AuthProvider")
  return context
}
