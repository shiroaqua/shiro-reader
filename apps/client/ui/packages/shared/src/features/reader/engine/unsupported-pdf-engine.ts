import { READER_ERROR_CODES } from "@shiro/ui-shared/api/reader-bridge"
import type {
  BookFormat,
  ReaderDocumentInstance,
  ReaderEngineAdapter,
} from "../contracts/engine"

export class UnsupportedFormatError extends Error {
  readonly code = READER_ERROR_CODES.UNSUPPORTED_FORMAT
  readonly key = "client.reader.unsupported_format" as const
  readonly format: BookFormat | "UNKNOWN"

  constructor(format: BookFormat | "UNKNOWN") {
    super(`Unsupported reader format: ${format}`)
    this.name = "UnsupportedFormatError"
    this.format = format
  }
}

export const unsupportedPdfEngine: ReaderEngineAdapter = {
  format: "PDF",
  async open(): Promise<ReaderDocumentInstance> {
    throw new UnsupportedFormatError("PDF")
  },
}
