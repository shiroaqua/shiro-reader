// Formats an ISO timestamp as YYYY/MM/DD in local time, regardless of locale.
export function formatBookDate(value: string) {
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return ""
  const month = String(date.getMonth() + 1).padStart(2, "0")
  const day = String(date.getDate()).padStart(2, "0")
  return `${date.getFullYear()}/${month}/${day}`
}
