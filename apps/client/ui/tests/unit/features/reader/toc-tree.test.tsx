import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import {
  resetReaderBridge,
  setReaderBridgeForTest,
} from "@shiro/ui-shared/api/reader-bridge"
import { createMockReaderBridge } from "@shiro/ui-shared/api/reader-mock"
import {
  ExternalLinkConfirmDialog,
  isSafeExternalUrl,
} from "@shiro/ui-shared/features/reader/components/external-link-confirm-dialog"
import { TocTree } from "@shiro/ui-shared/features/reader/components/toc-tree"
import {
  TOC_FLAG_DEFAULT_COLLAPSED,
  TOC_FLAG_EXTERNAL_LINK,
  type ReaderTocNode,
} from "@shiro/ui-shared/features/reader/contracts/engine"
import { InWindowTabStore } from "@shiro/ui-shared/features/reader/workspace/tab-session-store"

const SAMPLE_TOC: readonly ReaderTocNode[] = [
  {
    parent: null,
    depth: 0,
    subtree_end: 3,
    title: "Part One (Collapsed)",
    locator: {
      kind: "epub",
      spine_index: 0,
      href: "part1.xhtml",
      fragment: null,
      cfi: null,
    },
    unit_index: 0,
    unit_offset: 0,
    start_progression: 0,
    flags: TOC_FLAG_DEFAULT_COLLAPSED,
  },
  {
    parent: 0,
    depth: 1,
    subtree_end: 2,
    title: "Chapter 1.1",
    locator: {
      kind: "epub",
      spine_index: 1,
      href: "ch1.xhtml",
      fragment: null,
      cfi: null,
    },
    unit_index: 1,
    unit_offset: 0,
    start_progression: 0.25,
    flags: 0,
  },
  {
    parent: 0,
    depth: 1,
    subtree_end: 3,
    title: "Chapter 1.2",
    locator: {
      kind: "epub",
      spine_index: 2,
      href: "ch2.xhtml",
      fragment: null,
      cfi: null,
    },
    unit_index: 2,
    unit_offset: 0,
    start_progression: 0.5,
    flags: 0,
  },
  {
    parent: null,
    depth: 0,
    subtree_end: 4,
    title: "External Reference",
    locator: null,
    unit_index: null,
    unit_offset: 0,
    start_progression: null,
    flags: TOC_FLAG_EXTERNAL_LINK,
  },
]

describe("TocTree flat Arena navigation", () => {
  it("respects DEFAULT_COLLAPSED, expands/collapses subtree via subtree_end, and records jump history on click", () => {
    const store = new InWindowTabStore()
    const { tab } = store.openOrActivate({
      id: "book-1",
      title: "Book 1",
      hash: "hash-1",
      type: "EPUB",
      created_at: "2026-01-01T00:00:00Z",
      updated_at: "2026-01-01T00:00:00Z",
    })
    expect(tab).not.toBeNull()

    const goToLocator = vi.fn()
    const onExternalLink = vi.fn()

    render(
      <TocTree
        toc={SAMPLE_TOC}
        activeTocIndex={0}
        onExternalLink={onExternalLink}
        onSelectNode={(node) => {
          if (node.locator) {
            store.pushHistory(tab!.tabId, {
              locator: {
                kind: "epub",
                spine_index: 0,
                href: "part1.xhtml",
                fragment: null,
                cfi: "epubcfi(/6/2!)",
              },
              fraction: 0.1,
            })
            goToLocator(node.locator)
          }
        }}
      />
    )

    // Children of DEFAULT_COLLAPSED node are hidden initially
    expect(screen.getByText("Part One (Collapsed)")).toBeInTheDocument()
    expect(screen.queryByText("Chapter 1.1")).not.toBeInTheDocument()
    expect(screen.queryByText("Chapter 1.2")).not.toBeInTheDocument()
    expect(screen.getByText("External Reference")).toBeInTheDocument()

    // Expand subtree
    fireEvent.click(screen.getByRole("button", { name: /Expand section|展开子章节/i }))
    expect(screen.getByText("Chapter 1.1")).toBeInTheDocument()
    expect(screen.getByText("Chapter 1.2")).toBeInTheDocument()

    // Click Chapter 1.2 -> calls goToLocator and pushes history
    fireEvent.click(screen.getByText("Chapter 1.2"))
    expect(goToLocator).toHaveBeenCalledWith(SAMPLE_TOC[2]!.locator)
    expect(store.getSnapshot().tabs[0]!.history).toHaveLength(1)

    // External node does not crash or call goToLocator
    fireEvent.click(screen.getByText("External Reference"))
    expect(onExternalLink).toHaveBeenCalledWith("External Reference")
    expect(goToLocator).toHaveBeenCalledTimes(1)

    // Collapse subtree again
    fireEvent.click(screen.getByRole("button", { name: /Collapse section|收起子章节/i }))
    expect(screen.queryByText("Chapter 1.1")).not.toBeInTheDocument()
  })

  it("shows active-descendant indicator on collapsed parent and passes external_url when available", () => {
    const onExternalLink = vi.fn()
    const tocWithExternalUrl: readonly ReaderTocNode[] = [
      ...SAMPLE_TOC.slice(0, 3),
      {
        ...SAMPLE_TOC[3]!,
        external_url: "https://example.com/reference",
      },
    ]

    render(
      <TocTree
        toc={tocWithExternalUrl}
        activeTocIndex={2}
        onSelectNode={vi.fn()}
        onExternalLink={onExternalLink}
      />
    )

    // Parent node 0 is collapsed by default while activeTocIndex=2 is inside its subtree [1, 3).
    const parentBtn = screen.getByRole("button", { name: /Part One \(Collapsed\)/ })
    expect(parentBtn).toHaveAttribute("data-has-active-descendant", "true")
    expect(screen.getByTestId("toc-collapsed-active-dot")).toBeInTheDocument()

    // Expanding parent clears the collapsed active-descendant indicator and highlights Chapter 1.2 directly.
    fireEvent.click(screen.getByRole("button", { name: /Expand section|展开子章节/i }))
    expect(parentBtn).not.toHaveAttribute("data-has-active-descendant")
    expect(screen.queryByTestId("toc-collapsed-active-dot")).not.toBeInTheDocument()

    // Clicking external TOC entry with external_url forwards (url, title).
    fireEvent.click(screen.getByText("External Reference"))
    expect(onExternalLink).toHaveBeenCalledWith(
      "https://example.com/reference",
      "External Reference"
    )
  })

  it("confirms safe external links via ExternalLinkConfirmDialog and blocks unsafe schemes", async () => {
    expect(isSafeExternalUrl("https://example.com/path?a=1")).toBe(true)
    expect(isSafeExternalUrl("mailto:hello@example.com")).toBe(true)
    expect(isSafeExternalUrl("javascript:alert(1)")).toBe(false)
    expect(isSafeExternalUrl("file:///etc/passwd")).toBe(false)
    expect(isSafeExternalUrl("https://example.com/\u202Eevil")).toBe(false)
    expect(
      isSafeExternalUrl('mailto:a@b.com" /a "\\\\attacker\\share\\payload')
    ).toBe(false)
    expect(isSafeExternalUrl("mailto:a@b.com /arg")).toBe(false)

    const mockBridge = createMockReaderBridge()
    const openExternalSpy = vi
      .spyOn(mockBridge, "readerOpenExternalUrl")
      .mockResolvedValue(undefined)
    setReaderBridgeForTest(mockBridge)

    try {
      const onClose = vi.fn()
      const { rerender } = render(
        <ExternalLinkConfirmDialog
          link={{ url: "https://example.com/docs", title: "Documentation" }}
          onClose={onClose}
        />
      )

      const urlBox = screen.getByTestId("external-link-target-url")
      expect(urlBox).toHaveAttribute("dir", "ltr")
      expect(urlBox).toHaveTextContent("https://example.com/docs")
      const openBtn = screen.getByRole("button", {
        name: /Open in Browser|在浏览器中打开/i,
      })
      expect(openBtn).not.toBeDisabled()
      fireEvent.click(openBtn)
      await waitFor(() => {
        expect(openExternalSpy).toHaveBeenCalledWith("https://example.com/docs")
        expect(onClose).toHaveBeenCalledTimes(1)
      })

      // Punycode & userinfo spoofing normalization/highlighting
      rerender(
        <ExternalLinkConfirmDialog
          link={{ url: "https://trusted.com@例え.jp/login" }}
          onClose={onClose}
        />
      )
      expect(screen.getByTestId("external-link-userinfo")).toHaveTextContent(
        "trusted.com@"
      )
      expect(screen.getByTestId("external-link-host")).toHaveTextContent(
        "xn--r8jz45g.jp"
      )

      // Unsafe javascript: scheme disables confirmation action
      rerender(
        <ExternalLinkConfirmDialog
          link={{ url: "javascript:alert(1)" }}
          onClose={onClose}
        />
      )
      expect(
        screen.getByRole("button", { name: /Open in Browser|在浏览器中打开/i })
      ).toBeDisabled()
    } finally {
      resetReaderBridge()
    }
  })
})
