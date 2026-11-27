import {
  normalizeBookFormat,
  type BookFormat,
  type OpenDocumentParams,
  type ReaderDocumentInstance,
  type ReaderEngineAdapter,
} from "../contracts/engine"
import { foliateEpubEngine } from "./epub/foliate-epub-engine"
import {
  UnsupportedFormatError,
  unsupportedPdfEngine,
} from "./unsupported-pdf-engine"

export class ReaderEngineRegistry {
  private readonly adapters = new Map<BookFormat, ReaderEngineAdapter>()

  constructor(initialAdapters: readonly ReaderEngineAdapter[] = []) {
    for (const adapter of initialAdapters) {
      this.register(adapter)
    }
  }

  register(adapter: ReaderEngineAdapter): void {
    this.adapters.set(adapter.format, adapter)
  }

  get(format: BookFormat): ReaderEngineAdapter | undefined {
    return this.adapters.get(format)
  }

  has(format: BookFormat): boolean {
    return this.adapters.has(format)
  }

  async open(
    rawFormat: string,
    params: OpenDocumentParams
  ): Promise<ReaderDocumentInstance> {
    const normalized = normalizeBookFormat(rawFormat)
    if (normalized === "UNKNOWN") {
      throw new UnsupportedFormatError("UNKNOWN")
    }
    const adapter = this.adapters.get(normalized)
    if (!adapter) {
      throw new UnsupportedFormatError(normalized)
    }
    return adapter.open(params)
  }
}

export const readerEngineRegistry = new ReaderEngineRegistry([
  foliateEpubEngine,
  unsupportedPdfEngine,
])
