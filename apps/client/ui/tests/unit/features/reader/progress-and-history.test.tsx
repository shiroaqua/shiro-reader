import { useState } from "react"
import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { ProgressScrubber } from "@shiro/ui-shared/features/reader/components/progress-scrubber"
import type { BookLocator } from "@shiro/ui-shared/features/reader/contracts/engine"
import {
  InWindowTabStore,
  NavigationHistoryStack,
} from "@shiro/ui-shared/features/reader/workspace/tab-session-store"

describe("ProgressScrubber and NavigationHistoryStack", () => {
  it("caps NavigationHistoryStack at 16 entries and pops in LIFO order", () => {
    const stack = new NavigationHistoryStack(16)
    for (let i = 0; i < 20; i++) {
      stack.push({
        locator: {
          kind: "epub",
          spine_index: i,
          href: `ch${i}.xhtml`,
          fragment: null,
          cfi: null,
        },
        fraction: i / 20,
      })
    }
    expect(stack.size).toBe(16)
    const top = stack.peek()
    expect(top?.locator.kind === "epub" ? top.locator.spine_index : null).toBe(19)
    expect(stack.pop()?.fraction).toBeCloseTo(19 / 20)
    expect(stack.size).toBe(15)
  })

  it("previews slider changes while dragging, commits only on release, and restores exact locator via Back pill", () => {
    const store = new InWindowTabStore()
    const { tab } = store.openOrActivate({
      id: "book-1",
      title: "Book 1",
      hash: "hash-1",
      type: "EPUB",
      created_at: "2026-01-01T00:00:00Z",
      updated_at: "2026-01-01T00:00:00Z",
    })
    const tabId = tab!.tabId

    const initialLocator: BookLocator = {
      kind: "epub",
      spine_index: 0,
      href: "ch0.xhtml",
      fragment: null,
      cfi: "epubcfi(/6/2!/4/2)",
    }

    const goToFractionSpy = vi.fn()
    const goToLocatorSpy = vi.fn()

    function Harness() {
      const [fraction, setFraction] = useState(0.14)
      const [historyCount, setHistoryCount] = useState(0)
      const [backFraction, setBackFraction] = useState<number | null>(null)

      return (
        <ProgressScrubber
          fraction={fraction}
          pageStats={{ current: 3, total: 18 }}
          activeChapterTitle="Chapter 1"
          canGoBack={historyCount > 0}
          backFraction={backFraction}
          onCommitFraction={(nextFraction) => {
            store.pushHistory(tabId, {
              locator: initialLocator,
              fraction,
            })
            setHistoryCount(store.getSnapshot().tabs[0]!.history.length)
            setBackFraction(fraction)
            setFraction(nextFraction)
            goToFractionSpy(nextFraction)
          }}
          onGoBack={() => {
            const prev = store.popHistory(tabId)
            setHistoryCount(store.getSnapshot().tabs[0]!.history.length)
            if (prev) {
              setFraction(prev.fraction)
              goToLocatorSpy(prev.locator)
            }
          }}
        />
      )
    }

    render(<Harness />)

    expect(screen.queryByTestId("reader-history-back-pill")).not.toBeInTheDocument()
    expect(screen.getByTestId("reader-page-stats")).toHaveTextContent(/3\/18/)
    expect(screen.getByTestId("reader-progress-percent")).toHaveTextContent("14.0%")

    const slider = screen.getByRole("slider")
    // Dragging updates preview text without committing
    fireEvent.change(slider, { target: { value: "625" } })
    expect(screen.getByTestId("reader-progress-percent")).toHaveTextContent("62.5%")
    expect(goToFractionSpy).not.toHaveBeenCalled()

    // Releasing commits once, pushes history, and displays Back pill
    fireEvent.pointerUp(slider)
    expect(goToFractionSpy).toHaveBeenCalledTimes(1)
    expect(goToFractionSpy).toHaveBeenCalledWith(0.625)

    const backPill = screen.getByTestId("reader-history-back-pill")
    expect(backPill).toBeInTheDocument()

    // Clicking Back restores original BookLocator and hides pill
    fireEvent.click(backPill)
    expect(goToLocatorSpy).toHaveBeenCalledWith(initialLocator)
    expect(screen.queryByTestId("reader-history-back-pill")).not.toBeInTheDocument()
    expect(screen.getByTestId("reader-progress-percent")).toHaveTextContent("14.0%")
  })
})
