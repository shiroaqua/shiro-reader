import { useEffect, useRef, useState } from "react"
import { Check, LoaderCircle, X } from "lucide-react"
import { Input } from "@/components/ui/input"
import { useAppRuntime } from "@/context/app-runtime-context"
import { translateLocal, useTranslation } from "@/lib/i18n"
import { cn } from "cn"

type CheckState = {
  target: string
  status: "checking" | "valid" | "invalid"
  message: string | null
}

const DEBOUNCE_MS = 450

/**
 * Cloud server origin input that validates URL syntax and probes `/api/v1/meta`
 * connectivity via the Rust runtime, showing a green check on success or red X
 * on failure on the right side of the input.
 */
export function CloudOriginInput({
  id,
  value,
  onChange,
  onValidityChange,
  disabled = false,
  placeholder,
  className,
}: {
  id?: string
  value: string
  onChange: (value: string) => void
  onValidityChange: (valid: boolean) => void
  disabled?: boolean
  placeholder?: string
  className?: string
}) {
  const { t, locale } = useTranslation()
  const { cloudCheckOrigin } = useAppRuntime()
  const [checkState, setCheckState] = useState<CheckState | null>(null)
  const [checkNonce, setCheckNonce] = useState(0)
  const immediateNextRef = useRef(value.trim().length > 0)
  const onValidityChangeRef = useRef(onValidityChange)

  useEffect(() => {
    onValidityChangeRef.current = onValidityChange
  }, [onValidityChange])

  const trimmed = value.trim()

  useEffect(() => {
    if (!trimmed) {
      immediateNextRef.current = false
      onValidityChangeRef.current(false)
      return
    }

    let cancelled = false
    const delay = immediateNextRef.current ? 0 : DEBOUNCE_MS
    immediateNextRef.current = false

    const timer = window.setTimeout(() => {
      if (cancelled) return
      setCheckState({ target: trimmed, status: "checking", message: null })
      onValidityChangeRef.current(false)

      void cloudCheckOrigin(trimmed)
        .then((result) => {
          if (cancelled) return
          setCheckState({
            target: trimmed,
            status: result.ok ? "valid" : "invalid",
            message: result.ok ? null : result.message,
          })
          onValidityChangeRef.current(result.ok)
        })
        .catch(() => {
          if (cancelled) return
          setCheckState({
            target: trimmed,
            status: "invalid",
            message: translateLocal("local.cloud.origin_invalid", locale),
          })
          onValidityChangeRef.current(false)
        })
    }, delay)

    return () => {
      cancelled = true
      window.clearTimeout(timer)
    }
  }, [trimmed, locale, checkNonce, cloudCheckOrigin])

  const status: "idle" | "checking" | "valid" | "invalid" = !trimmed
    ? "idle"
    : checkState && checkState.target === trimmed
      ? checkState.status
      : "checking"

  const errorMessage =
    status === "invalid" && checkState?.target === trimmed
      ? checkState.message
      : null

  const triggerImmediateCheck = () => {
    if (!trimmed) return
    if (checkState?.target === trimmed && checkState.status === "valid") return
    immediateNextRef.current = true
    setCheckNonce((n) => n + 1)
  }

  return (
    <div className="min-w-0 flex-1 space-y-1">
      <div className="relative">
        <Input
          id={id}
          type="url"
          placeholder={placeholder ?? t("local.setup.cloud_origin_placeholder")}
          value={value}
          onChange={(e) => {
            const next = e.target.value
            onChange(next)
            onValidityChange(false)
            if (!next.trim()) {
              setCheckState(null)
            }
          }}
          onBlur={triggerImmediateCheck}
          disabled={disabled}
          aria-invalid={status === "invalid" ? true : undefined}
          className={cn("pr-9", className)}
        />
        {status !== "idle" && (
          <span className="pointer-events-none absolute inset-y-0 right-2.5 flex items-center">
            {status === "checking" && (
              <LoaderCircle
                aria-label={t("local.cloud.origin_checking")}
                className="size-4 animate-spin text-muted-foreground"
              />
            )}
            {status === "valid" && (
              <Check
                aria-label={t("local.cloud.origin_valid")}
                className="size-4 text-emerald-600 dark:text-emerald-400"
              />
            )}
            {status === "invalid" && (
              <X
                aria-label={t("local.cloud.origin_invalid")}
                className="size-4 text-destructive"
              />
            )}
          </span>
        )}
      </div>
      {errorMessage && (
        <p className="text-xs text-destructive" role="alert">
          {errorMessage}
        </p>
      )}
    </div>
  )
}
