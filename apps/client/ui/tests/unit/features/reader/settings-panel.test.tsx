import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { setReaderBridgeForTest } from "@shiro/ui-shared/api/reader-bridge"
import { createMockReaderBridge } from "@shiro/ui-shared/api/reader-mock"
import { FontPickerModal } from "@shiro/ui-shared/features/reader/components/settings-panel/FontPickerModal"
import { ReaderSettingsPanel } from "@shiro/ui-shared/features/reader/components/settings-panel/ReaderSettingsPanel"
import type { EngineCapabilities } from "@shiro/ui-shared/features/reader/contracts/engine"
import {
  getDefaultReaderProfile,
  READER_SETTING_BOUNDS,
} from "@shiro/ui-shared/features/reader/settings/defaults"
import {
  ensureReaderSettingsLoaded,
  flushPendingReaderSettingsWrites,
} from "@shiro/ui-shared/features/reader/settings/reader-settings-store"

const REFLOW_CAPS: EngineCapabilities = {
  supportsReflowTypography: true,
  supportsPublisherStyleOverride: true,
  supportsFlowSwitch: true,
  supportsColumnSwitch: true,
  supportsTextSelection: true,
  supportsToc: true,
  supportsFixedZoom: false,
  supportsFullTextTranslation: true,
  supportsBilingualInterleave: true,
}

const FIXED_LAYOUT_CAPS: EngineCapabilities = {
  supportsReflowTypography: false,
  supportsPublisherStyleOverride: false,
  supportsFlowSwitch: false,
  supportsColumnSwitch: false,
  supportsTextSelection: true,
  supportsToc: true,
  supportsFixedZoom: true,
  supportsFullTextTranslation: false,
  supportsBilingualInterleave: false,
}

describe("ReaderSettingsPanel", () => {
  it("writes to global vs book override methods according to active scope and resets book overrides", async () => {
    vi.useFakeTimers()
    const bridge = createMockReaderBridge()
    const updateGlobalSpy = vi.spyOn(bridge, "readerUpdateGlobalSettings")
    const setOverrideSpy = vi.spyOn(bridge, "readerSetBookOverride")
    const resetOverrideSpy = vi.spyOn(bridge, "readerResetBookOverride")
    setReaderBridgeForTest(bridge)

    await ensureReaderSettingsLoaded()

    render(
      <ReaderSettingsPanel bookHash="book-hash-xyz" capabilities={REFLOW_CAPS} />
    )

    // 1. Global scope edits update global settings
    expect(getDefaultReaderProfile(false).behavior.tapZonesEnabled).toBe(false)
    expect(getDefaultReaderProfile(true).behavior.tapZonesEnabled).toBe(false)
    expect(getDefaultReaderProfile(false).layout.pageMarginInlinePx).toBe(
      READER_SETTING_BOUNDS.pageMarginInlinePx.min
    )
    expect(getDefaultReaderProfile(false).layout.pageMarginBlockPx).toBe(
      READER_SETTING_BOUNDS.pageMarginBlockPx.min
    )
    expect(getDefaultReaderProfile(true).layout.pageMarginInlinePx).toBe(
      READER_SETTING_BOUNDS.pageMarginInlinePx.min
    )
    expect(getDefaultReaderProfile(true).layout.pageMarginBlockPx).toBe(
      READER_SETTING_BOUNDS.pageMarginBlockPx.min
    )
    expect(
      screen.getByRole("checkbox", {
        name: /Tap Edges to Turn Page|轻点左右边缘翻页/i,
      })
    ).not.toBeChecked()
    expect(
      screen.getByRole("checkbox", {
        name: /Show Page Number|显示页码/i,
      })
    ).toBeChecked()

    const fontSizeSlider = screen.getByRole("slider", {
      name: /Font Size|字号/i,
    })
    fireEvent.change(fontSizeSlider, { target: { value: "22" } })
    await vi.advanceTimersByTimeAsync(200)
    await flushPendingReaderSettingsWrites()

    expect(updateGlobalSpy).toHaveBeenCalledTimes(1)
    expect(updateGlobalSpy.mock.calls[0]?.[0].typography.fontSizePx).toBe(22)
    expect(setOverrideSpy).not.toHaveBeenCalled()

    // 2. Switch to Current Book scope -> writes book override
    fireEvent.click(screen.getByRole("tab", { name: /Current Book|仅当前书籍/i }))
    fireEvent.change(fontSizeSlider, { target: { value: "28" } })
    await vi.advanceTimersByTimeAsync(200)
    await flushPendingReaderSettingsWrites()

    expect(setOverrideSpy).toHaveBeenCalledWith("book-hash-xyz", {
      typography: { fontSizePx: 28 },
    })
    expect(
      screen.getByTestId("override-dot-typography-fontSizePx")
    ).toBeInTheDocument()

    // 3. Reset to global
    fireEvent.click(
      screen.getByRole("button", { name: /Reset to Global|恢复跟随全局/i })
    )
    expect(resetOverrideSpy).toHaveBeenCalledWith("book-hash-xyz")

    // 4. Toggle Show Page Number back in global scope
    fireEvent.click(screen.getByRole("tab", { name: /Global Defaults|全局默认/i }))
    const showPageNumberCheckbox = screen.getByRole("checkbox", {
      name: /Show Page Number|显示页码/i,
    })
    fireEvent.click(showPageNumberCheckbox)
    expect(showPageNumberCheckbox).not.toBeChecked()
    await vi.advanceTimersByTimeAsync(200)
    await flushPendingReaderSettingsWrites()
    expect(updateGlobalSpy.mock.calls.at(-1)?.[0].behavior.showPageNumber).toBe(
      false
    )
  })

  it("disables reflow typography, flow, and column controls under fixed-layout capabilities", async () => {
    setReaderBridgeForTest(createMockReaderBridge())
    await ensureReaderSettingsLoaded()

    render(
      <ReaderSettingsPanel
        bookHash="fxl-book-hash"
        capabilities={FIXED_LAYOUT_CAPS}
      />
    )

    expect(
      screen.getByText(/Fixed-layout books|固定版式书籍/i)
    ).toBeInTheDocument()
    expect(
      screen.getByRole("slider", { name: /Font Size|字号/i })
    ).toBeDisabled()
    expect(
      screen.getByRole("slider", { name: /Line Height|行高/i })
    ).toBeDisabled()
    expect(
      screen.getByRole("button", { name: /Scrolled|滚动/i })
    ).toBeDisabled()
    expect(
      screen.getByRole("button", { name: /2 Columns|双栏/i })
    ).toBeDisabled()
    expect(
      screen.getByRole("checkbox", { name: /Page Turn Animation|翻页动画/i })
    ).toBeDisabled()
    expect(
      screen.getByRole("checkbox", {
        name: /Seamless Chapter Scroll|无缝连续滚动/i,
      })
    ).toBeDisabled()
    expect(
      screen.queryByRole("checkbox", {
        name: /Mouse Wheel Turns Page|滚轮翻页/i,
      })
    ).not.toBeInTheDocument()
  })

  it("keeps Global Defaults uncontaminated by book overrides, supports paragraphIndentEm & checkbox reset, and disables paginated-only controls in scrolled flow", async () => {
    vi.useFakeTimers()
    setReaderBridgeForTest(createMockReaderBridge())
    await ensureReaderSettingsLoaded()

    render(
      <ReaderSettingsPanel bookHash="book-hash-iso" capabilities={REFLOW_CAPS} />
    )

    const fontSizeSlider = screen.getByRole("slider", {
      name: /Font Size|字号/i,
    }) as HTMLInputElement
    fireEvent.change(fontSizeSlider, { target: { value: "20" } })
    expect(fontSizeSlider.value).toBe("20")

    // Switch to Current Book -> override fontSizePx & checkbox overridePublisherStyles & paragraphIndentEm
    fireEvent.click(screen.getByRole("tab", { name: /Current Book|仅当前书籍/i }))
    fireEvent.change(fontSizeSlider, { target: { value: "28" } })
    expect(fontSizeSlider.value).toBe("28")

    const overrideCheckbox = screen.getByRole("checkbox", {
      name: /Override Publisher Styles|覆盖书籍自带排版/i,
    })
    fireEvent.click(overrideCheckbox)
    expect(
      screen.getByTestId("override-dot-typography-overridePublisherStyles")
    ).toBeInTheDocument()

    // Per-field reset on checkbox removes its override dot
    fireEvent.click(
      screen.getByRole("button", {
        name: /Reset.*Override Publisher Styles|恢复默认.*覆盖书籍自带排版/i,
      })
    )
    expect(
      screen.queryByTestId("override-dot-typography-overridePublisherStyles")
    ).not.toBeInTheDocument()

    // Paragraph indent slider & auto toggle
    const indentSlider = screen.getByRole("slider", {
      name: /Paragraph Indent|首行缩进/i,
    }) as HTMLInputElement
    fireEvent.change(indentSlider, { target: { value: "2" } })
    expect(indentSlider.value).toBe("2")

    // Switch back to Global Defaults -> still displays 20, NOT book override 28
    fireEvent.click(screen.getByRole("tab", { name: /Global Defaults|全局默认/i }))
    expect(fontSizeSlider.value).toBe("20")

    const seamlessCheckbox = screen.getByRole("checkbox", {
      name: /Seamless Chapter Scroll|无缝连续滚动/i,
    })
    expect(seamlessCheckbox).not.toBeChecked()
    expect(seamlessCheckbox).toBeDisabled()

    // Switch to Scrolled flow -> paginated-only controls disabled, seamless scroll enabled
    fireEvent.click(screen.getByRole("button", { name: /Scrolled|滚动/i }))
    expect(
      screen.getByRole("button", { name: /2 Columns|双栏/i })
    ).toBeDisabled()
    expect(
      screen.getByRole("checkbox", { name: /Page Turn Animation|翻页动画/i })
    ).toBeDisabled()
    expect(seamlessCheckbox).not.toBeDisabled()
    fireEvent.click(seamlessCheckbox)
    expect(seamlessCheckbox).toBeChecked()
  })

  it("propagates dark surface CSS variables to portaled FontPickerModal", async () => {
    setReaderBridgeForTest(createMockReaderBridge())
    await ensureReaderSettingsLoaded()

    render(
      <ReaderSettingsPanel
        bookHash="dark-surface-hash"
        capabilities={REFLOW_CAPS}
        surfaceCssVars={{
          "--background": "#0b0c0e",
          "--popover": "#0b0c0e",
          "--foreground": "#c9ccd4",
        }}
        isDarkSurface={true}
      />
    )

    fireEvent.click(
      screen.getByRole("button", { name: /Publisher Default|书籍原定/i })
    )
    const dialog = screen.getByRole("dialog")
    expect(dialog).toHaveAttribute("data-dark-surface", "true")
    expect(dialog.style.getPropertyValue("--popover")).toBe("#0b0c0e")
  })

  it("resets global defaults to platform defaults while preserving per-book overrides", async () => {
    const bridge = createMockReaderBridge()
    const updateGlobalSpy = vi.spyOn(bridge, "readerUpdateGlobalSettings")
    setReaderBridgeForTest(bridge)
    await ensureReaderSettingsLoaded()

    const { rerender } = render(
      <ReaderSettingsPanel
        bookHash="book-hash-reset"
        capabilities={REFLOW_CAPS}
        isMobile={false}
      />
    )

    const fontSizeSlider = screen.getByRole("slider", {
      name: /Font Size|字号/i,
    }) as HTMLInputElement
    fireEvent.change(fontSizeSlider, { target: { value: "26" } })
    expect(fontSizeSlider.value).toBe("26")

    // Add a per-book override on book-hash-reset
    fireEvent.click(screen.getByRole("tab", { name: /Current Book|仅当前书籍/i }))
    fireEvent.change(fontSizeSlider, { target: { value: "32" } })
    await flushPendingReaderSettingsWrites()

    // Switch back to Global scope and click Reset to Defaults
    fireEvent.click(screen.getByRole("tab", { name: /Global Defaults|全局默认/i }))
    fireEvent.click(
      screen.getByRole("button", { name: /Reset to Defaults|重置默认配置/i })
    )

    await vi.waitFor(() => {
      expect(updateGlobalSpy).toHaveBeenLastCalledWith(
        getDefaultReaderProfile(false)
      )
    })
    expect(fontSizeSlider.value).toBe("18")

    // Per-book override remains intact
    fireEvent.click(screen.getByRole("tab", { name: /Current Book|仅当前书籍/i }))
    expect(fontSizeSlider.value).toBe("32")

    // Mobile panel resets using mobile platform defaults
    rerender(
      <ReaderSettingsPanel
        bookHash="book-hash-reset"
        capabilities={REFLOW_CAPS}
        isMobile={true}
      />
    )
    fireEvent.click(screen.getByRole("tab", { name: /Global Defaults|全局默认/i }))
    fireEvent.click(
      screen.getByRole("button", { name: /Reset to Defaults|重置默认配置/i })
    )
    await vi.waitFor(() => {
      expect(updateGlobalSpy).toHaveBeenLastCalledWith(
        getDefaultReaderProfile(true)
      )
    })
  })

  it("virtualizes large system font lists in FontPickerModal, resets scrollTop on search, supports custom font actions, and hides system section when unsupported", () => {
    const systemFonts = Array.from({ length: 200 }, (_, i) => ({
      family: `SysFont-${String(i).padStart(3, "0")}`,
    }))
    const customFonts = [
      {
        fontId: "a".repeat(64),
        familyName: "My Imported Serif",
        fileName: "my-serif.ttf",
        format: "ttf" as const,
        sizeBytes: 4096,
      },
    ]
    const onSelect = vi.fn()
    const onOpenChange = vi.fn()
    const onImportCustomFont = vi.fn()
    const onDeleteCustomFont = vi.fn()

    const { rerender } = render(
      <FontPickerModal
        open={true}
        onOpenChange={onOpenChange}
        value={{ kind: "publisher" }}
        customFonts={customFonts}
        systemFonts={systemFonts}
        canEnumerateSystemFonts={true}
        onSelect={onSelect}
        onImportCustomFont={onImportCustomFont}
        onDeleteCustomFont={onDeleteCustomFont}
      />
    )

    // Virtualization: first items rendered, tail items NOT mounted in DOM simultaneously
    expect(screen.getByText("SysFont-000")).toBeInTheDocument()
    expect(screen.queryByText("SysFont-199")).not.toBeInTheDocument()

    // Scroll down to render tail items, then search -> resets scrollTop to 0 and filters results
    const scrollContainer = screen.getByText("SysFont-000").closest(".overflow-y-auto")!
    scrollContainer.scrollTop = 6400
    fireEvent.scroll(scrollContainer)
    expect(screen.getByText("SysFont-199")).toBeInTheDocument()

    const searchInput = screen.getByRole("textbox", {
      name: /Search fonts|搜索字体/i,
    })
    fireEvent.change(searchInput, { target: { value: "SysFont-142" } })
    expect(scrollContainer.scrollTop).toBe(0)
    expect(screen.getByText("SysFont-142")).toBeInTheDocument()
    expect(screen.queryByText("SysFont-000")).not.toBeInTheDocument()

    // Selecting a filtered system font calls onSelect + closes modal
    fireEvent.click(screen.getByText("SysFont-142"))
    expect(onSelect).toHaveBeenCalledWith({
      kind: "system",
      family: "SysFont-142",
    })
    expect(onOpenChange).toHaveBeenCalledWith(false)

    // Import & delete custom font controls work
    fireEvent.click(
      screen.getByRole("button", { name: /Import Font|导入字体/i })
    )
    expect(onImportCustomFont).toHaveBeenCalledTimes(1)
    fireEvent.click(
      screen.getByRole("button", { name: /Delete font My Imported Serif|删除字体 My Imported Serif/i })
    )
    expect(onDeleteCustomFont).toHaveBeenCalledWith("a".repeat(64))

    // canEnumerateSystemFonts=false hides system font search and list entirely (Android)
    rerender(
      <FontPickerModal
        open={true}
        onOpenChange={onOpenChange}
        value={{ kind: "publisher" }}
        customFonts={customFonts}
        systemFonts={systemFonts}
        canEnumerateSystemFonts={false}
        onSelect={onSelect}
        onImportCustomFont={onImportCustomFont}
        onDeleteCustomFont={onDeleteCustomFont}
      />
    )
    expect(
      screen.queryByRole("textbox", { name: /Search fonts|搜索字体/i })
    ).not.toBeInTheDocument()
  })
})
