import { createBLAKE3 } from "hash-wasm"

// The server identifies book files by their BLAKE3 hash, so it must be
// computed with the same algorithm. Streams the file to keep memory flat.
export async function hashFile(file: Blob): Promise<string> {
  const hasher = await createBLAKE3()
  hasher.init()
  const reader = file.stream().getReader()
  for (;;) {
    const { done, value } = await reader.read()
    if (done) break
    hasher.update(value)
  }
  return hasher.digest("hex")
}
