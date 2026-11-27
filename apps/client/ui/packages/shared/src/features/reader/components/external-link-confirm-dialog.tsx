import { useState, type CSSProperties } from "react"
import { ExternalLink } from "lucide-react"
import { readerBridge } from "@shiro/ui-shared/api/reader-bridge"
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@shiro/ui-shared/components/ui/alert-dialog"
import { Button } from "@shiro/ui-shared/components/ui/button"
import { useTranslation } from "@shiro/ui-shared/lib/i18n"

const MAX_EXTERNAL_URL_LENGTH = 4096

export interface PendingExternalLink {
  readonly url: string
  readonly title?: string | undefined
}

function isBidiOrControlCode(code: number): boolean {
  return (
    code <= 0x1f ||
    (code >= 0x7f && code <= 0x9f) ||
    code === 0x061c ||
    code === 0x200e ||
    code === 0x200f ||
    (code >= 0x202a && code <= 0x202e) ||
    (code >= 0x2066 && code <= 0x2069)
  )
}

function hasControlOrBidiChar(value: string): boolean {
  for (let i = 0; i < value.length; i++) {
    if (isBidiOrControlCode(value.charCodeAt(i))) {
      return true
    }
  }
  return false
}

function hasUnsafeExternalChar(value: string): boolean {
  for (let i = 0; i < value.length; i++) {
    const ch = value[i]!
    if (
      isBidiOrControlCode(ch.charCodeAt(0)) ||
      /\s/.test(ch) ||
      ch === '"' ||
      ch === "<" ||
      ch === ">" ||
      ch === "`" ||
      ch === "\\"
    ) {
      return true
    }
  }
  return false
}

function stripBidiAndControl(value: string): string {
  let out = ""
  for (let i = 0; i < value.length; i++) {
    if (!isBidiOrControlCode(value.charCodeAt(i))) {
      out += value[i]
    }
  }
  return out
}

export function parseSafeExternalUrl(rawUrl: string): URL | null {
  const trimmed = rawUrl.trim()
  if (
    trimmed.length === 0 ||
    trimmed.length > MAX_EXTERNAL_URL_LENGTH ||
    hasControlOrBidiChar(trimmed)
  ) {
    return null
  }
  try {
    const parsed = new URL(trimmed)
    if (
      parsed.href.length > MAX_EXTERNAL_URL_LENGTH ||
      hasUnsafeExternalChar(parsed.href)
    ) {
      return null
    }
    const protocol = parsed.protocol.toLowerCase()
    if (protocol === "http:" || protocol === "https:") {
      return parsed.hostname.length > 0 ? parsed : null
    }
    if (protocol === "mailto:") {
      return parsed.pathname.trim().length > 0 && !hasUnsafeExternalChar(trimmed)
        ? parsed
        : null
    }
    return null
  } catch {
    return null
  }
}

export function isSafeExternalUrl(rawUrl: string): boolean {
  return parseSafeExternalUrl(rawUrl) !== null
}

export interface ExternalLinkConfirmDialogProps {
  readonly link: PendingExternalLink | null
  readonly onClose: () => void
  readonly surfaceCssVars?: Readonly<Record<string, string>> | undefined
  readonly isDarkSurface?: boolean | undefined
}

export function ExternalLinkConfirmDialog({
  link,
  onClose,
  surfaceCssVars,
  isDarkSurface,
}: ExternalLinkConfirmDialogProps) {
  const { t } = useTranslation()
  const [opening, setOpening] = useState(false)
  const [openError, setOpenError] = useState<string | null>(null)
  const [prevLink, setPrevLink] = useState<PendingExternalLink | null>(link)

  if (link !== prevLink) {
    setPrevLink(link)
    setOpening(false)
    setOpenError(null)
  }

  const targetUrl = link?.url.trim() ?? ""
  const parsedUrl = link !== null ? parseSafeExternalUrl(targetUrl) : null
  const canOpen = parsedUrl !== null
  const userinfo =
    parsedUrl && (parsedUrl.username || parsedUrl.password)
      ? parsedUrl.password
        ? `${parsedUrl.username}:${parsedUrl.password}`
        : parsedUrl.username
      : null
  const safeDisplayFallback = stripBidiAndControl(targetUrl)

  const handleConfirmOpen = async () => {
    if (!parsedUrl || opening) return
    setOpening(true)
    setOpenError(null)
    try {
      await readerBridge.readerOpenExternalUrl(parsedUrl.href)
      onClose()
    } catch {
      setOpenError(t("local.reader.external_link.open_failed"))
      setOpening(false)
    }
  }

  return (
    <AlertDialog
      open={link !== null}
      onOpenChange={(open) => {
        if (!open) onClose()
      }}
    >
      <AlertDialogContent
        style={
          surfaceCssVars
            ? {
                ...(surfaceCssVars as CSSProperties),
                colorScheme: isDarkSurface ? "dark" : "light",
              }
            : undefined
        }
        data-dark-surface={
          isDarkSurface === undefined ? undefined : isDarkSurface ? "true" : "false"
        }
      >
        <AlertDialogHeader>
          <AlertDialogTitle className="flex items-center gap-2">
            <ExternalLink className="size-4 shrink-0" aria-hidden="true" />
            <span>{t("local.reader.external_link.title")}</span>
          </AlertDialogTitle>
          <AlertDialogDescription>
            {t("local.reader.external_link.description")}
          </AlertDialogDescription>
        </AlertDialogHeader>

        {link && (
          <div className="space-y-2">
            {link.title && link.title.trim() !== targetUrl && (
              <p className="text-xs font-medium text-foreground">{link.title}</p>
            )}
            <div
              data-testid="external-link-target-url"
              dir="ltr"
              style={{ unicodeBidi: "isolate" }}
              className="max-h-28 overflow-y-auto rounded-md border bg-muted/40 px-3 py-2 font-mono text-xs break-all text-foreground"
            >
              {parsedUrl &&
              (parsedUrl.protocol === "http:" || parsedUrl.protocol === "https:") ? (
                <>
                  {parsedUrl.protocol}//
                  {userinfo !== null && (
                    <span
                      data-testid="external-link-userinfo"
                      className="rounded bg-destructive/15 px-0.5 font-semibold text-destructive"
                    >
                      {userinfo}@
                    </span>
                  )}
                  <span
                    data-testid="external-link-host"
                    className="font-semibold text-foreground"
                  >
                    {parsedUrl.host}
                  </span>
                  {parsedUrl.pathname}
                  {parsedUrl.search}
                  {parsedUrl.hash}
                </>
              ) : parsedUrl ? (
                parsedUrl.href
              ) : (
                safeDisplayFallback
              )}
            </div>
            {!canOpen && (
              <p className="text-xs text-destructive">
                {t("local.reader.external_link.invalid_url")}
              </p>
            )}
            {openError && (
              <p role="alert" className="text-xs text-destructive">
                {openError}
              </p>
            )}
          </div>
        )}

        <AlertDialogFooter>
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={opening}
            onClick={onClose}
          >
            {t("local.reader.external_link.cancel")}
          </Button>
          <Button
            type="button"
            size="sm"
            disabled={!canOpen || opening}
            onClick={() => {
              void handleConfirmOpen()
            }}
          >
            <ExternalLink className="size-3.5" aria-hidden="true" />
            {t("local.reader.external_link.open")}
          </Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
