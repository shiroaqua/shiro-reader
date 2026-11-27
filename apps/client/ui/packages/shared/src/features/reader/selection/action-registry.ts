import type { EngineCapabilities } from "../contracts/engine"
import type {
  SelectionActionGroup,
  SelectionActionPlugin,
  SelectionContext,
} from "../contracts/selection"
import { copyActionPlugin } from "./plugins/copy-action"

const GROUP_PRIORITY: Readonly<Record<SelectionActionGroup, number>> = {
  clipboard: 0,
  annotation: 1,
  lookup: 2,
  ai: 3,
  share: 4,
}

export class SelectionActionRegistry {
  private readonly plugins = new Map<string, SelectionActionPlugin>()

  constructor(initialPlugins: readonly SelectionActionPlugin[] = []) {
    for (const plugin of initialPlugins) {
      this.register(plugin)
    }
  }

  register(plugin: SelectionActionPlugin): () => void {
    this.plugins.set(plugin.id, plugin)
    return () => {
      this.unregister(plugin.id)
    }
  }

  unregister(pluginId: string): void {
    this.plugins.delete(pluginId)
  }

  getById(pluginId: string): SelectionActionPlugin | undefined {
    return this.plugins.get(pluginId)
  }

  getAvailableActions(
    ctx: SelectionContext,
    capabilities: EngineCapabilities
  ): readonly SelectionActionPlugin[] {
    return Array.from(this.plugins.values())
      .filter((plugin) =>
        plugin.isAvailable ? plugin.isAvailable(ctx, capabilities) : true
      )
      .sort((a, b) => {
        const groupDiff = GROUP_PRIORITY[a.group] - GROUP_PRIORITY[b.group]
        if (groupDiff !== 0) return groupDiff
        return a.order - b.order
      })
  }
}

export const selectionActionRegistry = new SelectionActionRegistry([
  copyActionPlugin,
])
