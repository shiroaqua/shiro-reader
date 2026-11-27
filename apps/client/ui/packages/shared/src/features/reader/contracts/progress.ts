import type { UUID } from "@shiro/ui-shared/lib/types"
import type { BookLocator, RelocationSnapshot } from "./engine"

export interface ReadingProgressPort {
  loadInitialLocator(bookId: UUID, bookHash: string): Promise<BookLocator | undefined>
  reportRelocation(bookId: UUID, bookHash: string, snapshot: RelocationSnapshot): void
}

/** 初版默认实现：不跨进程持久化，仅依赖 Tab 内存态与跨窗口迁移载荷保持阅读位置 */
export const ephemeralProgressAdapter: ReadingProgressPort = {
  loadInitialLocator: async () => undefined,
  reportRelocation: () => {},
}
