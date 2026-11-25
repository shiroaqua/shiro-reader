// Formats an ISO timestamp as YYYY/MM/DD in local time, regardless of locale.
export function formatBookDate(value: string) {
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return ""
  const month = String(date.getMonth() + 1).padStart(2, "0")
  const day = String(date.getDate()).padStart(2, "0")
  return `${date.getFullYear()}/${month}/${day}`
}

const BYTE_UNITS = ["B", "KB", "MB", "GB", "TB"] as const

export function formatByteSize(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B"
  let value = bytes
  let unitIndex = 0
  while (value >= 1024 && unitIndex < BYTE_UNITS.length - 1) {
    value /= 1024
    unitIndex += 1
  }
  const unit = BYTE_UNITS[unitIndex] ?? "B"
  if (unitIndex === 0) return `${Math.round(value)} ${unit}`
  return `${value >= 100 ? value.toFixed(0) : value.toFixed(1)} ${unit}`
}

export function formatEtaSeconds(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 0) return "0s"
  const rounded = Math.ceil(seconds)
  if (rounded < 60) return `${rounded}s`
  const minutes = Math.floor(rounded / 60)
  const remSeconds = rounded % 60
  if (minutes < 60) {
    return remSeconds > 0 ? `${minutes}m ${remSeconds}s` : `${minutes}m`
  }
  const hours = Math.floor(minutes / 60)
  const remMinutes = minutes % 60
  return remMinutes > 0 ? `${hours}h ${remMinutes}m` : `${hours}h`
}
