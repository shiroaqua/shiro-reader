import { describe, expect, it } from "vitest"
import {
  applySuggestion,
  buildBookTagIndex,
  buildSuggestions,
  commitSearchDraft,
  formatTagToken,
  getSearchContext,
  matchesBookSearch,
  parseBookSearch,
  partitionSearchChips,
  removeSegment,
  removeTagSegments,
  tokenizeBookSearch,
} from "@/lib/book-search"
import { enTagNames } from "@/lib/locales/en/tags"
import { makeBook, makeBookTag, makeTagNode } from "../../support/builders"

function createSampleLibrary() {
  const authorTag = makeTagNode({
    id: "tag-author",
    name: "Author",
    color: "#75a9cc",
    allows_value: true,
  })
  const sciFiTag = makeTagNode({
    id: "tag-scifi",
    name: "SciFi",
    color: "#79b78f",
  })
  const spaceOperaTag = makeTagNode({
    id: "tag-space",
    name: "SpaceOpera",
    parent_id: "tag-scifi",
  })
  const isbnTag = makeTagNode({
    id: "tag-isbn",
    name: "ISBN",
    allows_value: true,
  })
  sciFiTag.children.push(spaceOperaTag)

  const dune = makeBook({ id: "b1", title: "Dune Messiah" })
  const leftHand = makeBook({ id: "b2", title: "The Left Hand of Darkness" })
  const foundation = makeBook({ id: "b3", title: "Foundation #2" })

  const index = buildBookTagIndex(
    [authorTag, sciFiTag, isbnTag],
    new Map([
      [
        dune.id,
        [makeBookTag(authorTag.id, "Frank Herbert"), makeBookTag(sciFiTag.id)],
      ],
      [
        leftHand.id,
        [
          makeBookTag(authorTag.id, "Ursula K. Le Guin"),
          makeBookTag(spaceOperaTag.id),
        ],
      ],
    ]),
    "en"
  )

  return {
    authorTag,
    sciFiTag,
    spaceOperaTag,
    isbnTag,
    dune,
    leftHand,
    foundation,
    books: [dune, leftHand, foundation],
    index,
  }
}

describe("book-search", () => {
  describe("tokenizeBookSearch and parseBookSearch", () => {
    it("parses free terms, bare tags, and quoted or unquoted tag values", () => {
      const parsed = parseBookSearch(
        'rust #SciFi #author:"Ursula K. Le Guin" #year:2024 guide'
      )

      expect(parsed).toEqual({
        terms: ["rust", "guide"],
        tagFilters: [
          { name: "SciFi", value: null },
          { name: "author", value: "Ursula K. Le Guin" },
          { name: "year", value: "2024" },
        ],
        isEmpty: false,
      })
    })

    it("unescapes quotes and backslashes inside quoted values and round-trips formatTagToken", () => {
      const formatted = formatTagToken("note", 'He said "hi" \\ bye')
      expect(formatted).toBe('#note:"He said \\"hi\\" \\\\ bye"')

      const parsed = parseBookSearch(formatted)
      expect(parsed.tagFilters).toEqual([
        { name: "note", value: 'He said "hi" \\ bye' },
      ])
    })

    it("excludes incomplete, unclosed, empty-value, or trailing-glued tag tokens from filters", () => {
      const query = '# #empty: #empty:"" #glued:"ok"tail #unclosed:"abc'
      const segments = tokenizeBookSearch(query)
      const tagSegments = segments.filter((segment) => segment.kind === "tag")

      expect(tagSegments).toHaveLength(5)
      expect(tagSegments.every((segment) => !segment.complete)).toBe(true)
      expect(parseBookSearch(query)).toEqual({
        terms: [],
        tagFilters: [],
        isEmpty: true,
      })
    })
  })

  describe("buildBookTagIndex and matchesBookSearch", () => {
    it("builds localized display names and falls back to raw tokenName when display name has spaces", () => {
      enTagNames["tag.series"] = "Series"
      enTagNames["tag.code"] = "Code / Ref"
      try {
        const seriesTag = makeTagNode({ id: "tag-series", name: "tag.series" })
        const codeTag = makeTagNode({ id: "tag-code", name: "tag.code" })
        const localizedIndex = buildBookTagIndex([seriesTag, codeTag], new Map(), "en")

        const series = localizedIndex.tags.find((entry) => entry.tag.id === "tag-series")
        const code = localizedIndex.tags.find((entry) => entry.tag.id === "tag-code")

        expect(series?.displayName).toBe("Series")
        expect(series?.tokenName).toBe("Series")
        expect(code?.displayName).toBe("Code / Ref")
        expect(code?.tokenName).toBe("tag.code")
      } finally {
        delete enTagNames["tag.series"]
        delete enTagNames["tag.code"]
      }
    })

    it("matches case-insensitive title words and exact tag/value filters with AND semantics", () => {
      const { dune, leftHand, foundation, index } = createSampleLibrary()

      // Case-insensitive title terms ANDed
      expect(
        matchesBookSearch(dune, parseBookSearch("dune messiah"), index)
      ).toBe(true)
      expect(
        matchesBookSearch(dune, parseBookSearch("dune foundation"), index)
      ).toBe(false)

      // Tag name & case-sensitive value
      expect(
        matchesBookSearch(
          dune,
          parseBookSearch('#Author:"Frank Herbert"'),
          index
        )
      ).toBe(true)
      expect(
        matchesBookSearch(
          dune,
          parseBookSearch('#Author:"frank herbert"'),
          index
        )
      ).toBe(false)

      // Exact tag match only (child SpaceOpera does not match parent SciFi filter)
      expect(matchesBookSearch(dune, parseBookSearch("#SciFi"), index)).toBe(
        true
      )
      expect(
        matchesBookSearch(leftHand, parseBookSearch("#SciFi"), index)
      ).toBe(false)

      // Book with unloaded bindings fails any tag filter
      expect(
        matchesBookSearch(foundation, parseBookSearch("#SciFi"), index)
      ).toBe(false)
    })
  })

  describe("getSearchContext", () => {
    it("identifies idle, text, tag-name, tag-value, and post-quote idle states", () => {
      expect(getSearchContext("dune ", 5)).toEqual({ mode: "idle", caret: 5 })
      expect(getSearchContext("dune", 4).mode).toBe("text")
      expect(getSearchContext("#Auth", 5)).toMatchObject({
        mode: "tag-name",
        prefix: "Auth",
      })
      expect(getSearchContext("#Author:", 8)).toMatchObject({
        mode: "tag-value",
        prefix: "",
      })
      expect(getSearchContext('#Author:"Fra', 12)).toMatchObject({
        mode: "tag-value",
        prefix: "Fra",
      })
      expect(getSearchContext('#Author:"Frank"', 15)).toEqual({
        mode: "idle",
        caret: 15,
      })
    })
  })

  describe("buildSuggestions and applySuggestion", () => {
    it("ranks tag suggestions by prefix match then usage count", () => {
      const { index, books } = createSampleLibrary()
      const context = getSearchContext("#", 1)

      const groups = buildSuggestions("#", context, index, books)
      expect(groups).toHaveLength(1)
      expect(groups[0]!.id).toBe("tags")
      expect(groups[0]!.items.map((item) => item.key)).toEqual([
        "tag:tag-author",
        "tag:tag-scifi",
        "tag:tag-space",
        "tag:tag-isbn",
      ])
    })

    it("offers any-value followed by stored values in tag-value mode", () => {
      const { index, books } = createSampleLibrary()
      const context = getSearchContext("#Author:", 8)

      const groups = buildSuggestions("#Author:", context, index, books)
      expect(groups).toHaveLength(1)
      expect(groups[0]!.id).toBe("values")
      expect(groups[0]!.items.map((item) => item.kind)).toEqual([
        "any-value",
        "value",
        "value",
      ])
    })

    it("inserts quoted template with caret inside quotes when selecting a valued tag", () => {
      const { index, books } = createSampleLibrary()
      const context = getSearchContext("#Auth", 5)
      const [group] = buildSuggestions("#Auth", context, index, books)
      const authorSuggestion = group!.items[0]!

      const edit = applySuggestion("#Auth", context, authorSuggestion, index)
      expect(edit).toEqual({
        query: '#Author:""',
        caret: '#Author:"'.length,
      })
    })

    it("inserts bare tag token when tag has no stored values and preserves tags when picking a book", () => {
      const { index, books } = createSampleLibrary()

      const tagContext = getSearchContext("#Sci", 4)
      const [tagGroup] = buildSuggestions("#Sci", tagContext, index, books)
      expect(applySuggestion("#Sci", tagContext, tagGroup!.items[0]!, index)).toEqual({
        query: "#SciFi ",
        caret: "#SciFi ".length,
      })

      const taggedBookContext = getSearchContext("#SciFi Dun", 10)
      const taggedGroups = buildSuggestions(
        "#SciFi Dun",
        taggedBookContext,
        index,
        books
      )
      const taggedBookGroup = taggedGroups.find((group) => group.id === "books")
      expect(taggedBookGroup).toBeDefined()

      const taggedBookEdit = applySuggestion(
        "#SciFi Dun",
        taggedBookContext,
        taggedBookGroup!.items[0]!,
        index
      )
      expect(taggedBookEdit.query).toBe("#SciFi Dune Messiah ")
      expect(taggedBookEdit.caret).toBe(taggedBookEdit.query.length)

      const hashTitleContext = getSearchContext("Found", 5)
      const hashTitleGroups = buildSuggestions(
        "Found",
        hashTitleContext,
        index,
        books
      )
      const hashTitleGroup = hashTitleGroups.find(
        (group) => group.id === "books"
      )
      expect(hashTitleGroup).toBeDefined()

      const hashTitleEdit = applySuggestion(
        "Found",
        hashTitleContext,
        hashTitleGroup!.items[0]!,
        index
      )
      expect(hashTitleEdit.query).toBe("Foundation 2 ")
      expect(hashTitleEdit.caret).toBe(hashTitleEdit.query.length)
    })
  })

  describe("segment removal and search chip partitioning", () => {
    it("removes single segments or all complete tag filters cleanly", () => {
      const query = '#SciFi dune #Author:"Frank Herbert" messiah'
      const segments = tokenizeBookSearch(query)

      expect(removeSegment(query, segments[0]!)).toBe(
        'dune #Author:"Frank Herbert" messiah'
      )
      expect(removeTagSegments(query)).toBe("dune messiah")
    })

    it("splits known complete tags into chips while keeping the actively edited tag in draft", () => {
      const { index } = createSampleLibrary()
      const value = '#SciFi #Author:"Frank Herbert" dune'

      const blurred = partitionSearchChips(value, index, false, null)
      expect(blurred.chips.map((chip) => chip.raw)).toEqual([
        "#SciFi",
        '#Author:"Frank Herbert"',
      ])
      expect(blurred.draft).toBe("dune")
      expect(blurred.prefixLength).toBe('#SciFi #Author:"Frank Herbert" '.length)

      const editingSecond = partitionSearchChips(value, index, true, 7)
      expect(editingSecond.chips.map((chip) => chip.raw)).toEqual(["#SciFi"])
      expect(editingSecond.draft).toBe('#Author:"Frank Herbert" dune')
    })

    it("promotes newly finished known tags in draft and adjusts caret and editingTagStart", () => {
      const { index } = createSampleLibrary()
      const existingChips = [{ raw: "#SciFi" }]
      const nextDraft = '#Author:"Frank Herbert" #SpaceOp dune'
      // Caret sits at end of `#SpaceOp`
      const caret = '#Author:"Frank Herbert" #SpaceOp'.length

      const committed = commitSearchDraft(existingChips, nextDraft, caret, index)

      expect(committed.promotedCount).toBe(1)
      expect(committed.query).toBe('#SciFi #Author:"Frank Herbert" #SpaceOp dune')
      expect(committed.draftCaret).toBe("#SpaceOp".length)
      expect(committed.editingTagStart).toBe(
        '#SciFi #Author:"Frank Herbert" '.length
      )
    })
  })
})
