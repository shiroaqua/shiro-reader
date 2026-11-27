import { useEffect, useMemo, useSyncExternalStore } from "react"
import {
  readerBridge,
  type ReaderBridge,
} from "@shiro/ui-shared/api/reader-bridge"
import { isTauriRuntime } from "@shiro/ui-shared/api/tauri-bridge"
import { isMobilePlatform } from "@shiro/ui-shared/hooks/use-mobile"
import type {
  ReaderProfile,
  ReaderProfilePatch,
  ReaderSettingsSnapshot,
  SystemFontFamily,
} from "../contracts/settings"
import {
  clearBookPatchField,
  mergeBookPatchField,
  pruneEmptyPatch,
  resolveEffectiveProfile,
} from "./cascade"
import { getDefaultReaderProfile } from "./defaults"

const WRITE_DEBOUNCE_MS = 200

const listeners = new Set<() => void>()
const sharedFontBuffersByBridge = new WeakMap<
  ReaderBridge,
  Map<string, Promise<ArrayBuffer>>
>()

function getFontBufferMap(
  bridge: ReaderBridge
): Map<string, Promise<ArrayBuffer>> {
  let map = sharedFontBuffersByBridge.get(bridge)
  if (!map) {
    map = new Map<string, Promise<ArrayBuffer>>()
    sharedFontBuffersByBridge.set(bridge, map)
  }
  return map
}

export function shouldUseFontFaceFallback(fontUrlBase: string): boolean {
  if (!isTauriRuntime()) return false
  if (!fontUrlBase.trim().toLowerCase().startsWith("shiro-font://")) {
    return false
  }
  if (typeof navigator === "undefined") return false
  const platformHint = `${navigator.platform ?? ""} ${navigator.userAgent ?? ""}`
  return /linux/i.test(platformHint)
}

export function loadSharedCustomFontBuffer(
  fontId: string,
  url: string,
  bridge: ReaderBridge = readerBridge
): Promise<ArrayBuffer> {
  const cache = getFontBufferMap(bridge)
  const existing = cache.get(fontId)
  if (existing) return existing

  const promise = bridge
    .readerReadCustomFont(fontId)
    .catch(async () => {
      const response = await fetch(url)
      if (!response.ok) {
        throw new Error(`Font fetch failed with status ${response.status}`)
      }
      return response.arrayBuffer()
    })
    .catch((err: unknown) => {
      if (cache.get(fontId) === promise) {
        cache.delete(fontId)
      }
      throw err
    })

  cache.set(fontId, promise)
  return promise
}

export function evictSharedCustomFontBuffer(
  fontId: string,
  bridge: ReaderBridge = readerBridge
): void {
  sharedFontBuffersByBridge.get(bridge)?.delete(fontId)
}

let snapshotState: ReaderSettingsSnapshot | null = null
let systemFontsState: readonly SystemFontFamily[] | null = null
let loadPromise: Promise<ReaderSettingsSnapshot> | null = null
let systemFontsPromise: Promise<readonly SystemFontFamily[]> | null = null
let unlistenEvents: (() => void) | null = null
let inferredIsMobile: boolean | null = null

let globalWriteRevision = 0
let globalFlushTimer: ReturnType<typeof setTimeout> | null = null
let pendingGlobalProfile: ReaderProfile | null = null
let customFontsRevision = 0
let fontMutationSettledRevision = 0

const bookWriteRevision = new Map<string, number>()
const bookFlushTimers = new Map<string, ReturnType<typeof setTimeout>>()
const pendingBookPatches = new Map<string, ReaderProfilePatch | null>()

const DESKTOP_FALLBACK_SNAPSHOT: ReaderSettingsSnapshot = {
  global: getDefaultReaderProfile(false),
  bookOverrides: {},
  customFonts: [],
  fontUrlBase: "shiro-font://localhost/font/",
  canEnumerateSystemFonts: true,
}

const MOBILE_FALLBACK_SNAPSHOT: ReaderSettingsSnapshot = {
  global: getDefaultReaderProfile(true),
  bookOverrides: {},
  customFonts: [],
  fontUrlBase: "http://shiro-font.localhost/font/",
  canEnumerateSystemFonts: false,
}

function normalizeBookHashKey(bookHash: string): string {
  return bookHash.trim().toLowerCase()
}

function resolveDefaultIsMobile(explicit?: boolean): boolean {
  return explicit ?? inferredIsMobile ?? isMobilePlatform()
}

function notifyListeners(): void {
  for (const listener of listeners) {
    listener()
  }
}

function getFallbackSnapshot(isMobile?: boolean): ReaderSettingsSnapshot {
  return resolveDefaultIsMobile(isMobile)
    ? MOBILE_FALLBACK_SNAPSHOT
    : DESKTOP_FALLBACK_SNAPSHOT
}

function sanitizePendingFontReferences(validFontIds: ReadonlySet<string>): void {
  if (
    pendingGlobalProfile?.typography.font.kind === "custom" &&
    !validFontIds.has(pendingGlobalProfile.typography.font.fontId)
  ) {
    pendingGlobalProfile = {
      ...pendingGlobalProfile,
      typography: {
        ...pendingGlobalProfile.typography,
        font: { kind: "publisher" },
      },
    }
  }
  for (const [hash, patch] of [...pendingBookPatches.entries()]) {
    if (
      patch?.typography?.font?.kind === "custom" &&
      !validFontIds.has(patch.typography.font.fontId)
    ) {
      pendingBookPatches.set(hash, {
        ...patch,
        typography: {
          ...patch.typography,
          font: { kind: "publisher" },
        },
      })
    }
  }
}

function evictRemovedFontBuffers(validFontIds: ReadonlySet<string>): void {
  const cache = sharedFontBuffersByBridge.get(readerBridge)
  if (!cache) return
  for (const cachedId of [...cache.keys()]) {
    if (!validFontIds.has(cachedId)) {
      cache.delete(cachedId)
    }
  }
}

function mergeIncomingSnapshot(
  incoming: ReaderSettingsSnapshot
): ReaderSettingsSnapshot {
  const validFontIds = new Set(incoming.customFonts.map((f) => f.fontId))
  sanitizePendingFontReferences(validFontIds)
  evictRemovedFontBuffers(validFontIds)

  const nextBookOverrides: Record<string, ReaderProfilePatch> = {
    ...incoming.bookOverrides,
  }
  for (const [bookHash, pendingPatch] of pendingBookPatches.entries()) {
    if (pendingPatch === null) {
      delete nextBookOverrides[bookHash]
    } else {
      nextBookOverrides[bookHash] = pendingPatch
    }
  }
  return {
    ...incoming,
    global: pendingGlobalProfile ?? incoming.global,
    bookOverrides: nextBookOverrides,
  }
}

function ensureSubscribedAndLoaded(): void {
  if (!unlistenEvents) {
    unlistenEvents = () => {}
    void readerBridge
      .listenReaderSettings((incoming) => {
        snapshotState = mergeIncomingSnapshot(incoming)
        notifyListeners()
      })
      .then((unlisten) => {
        if (listeners.size === 0 && !snapshotState) {
          unlisten()
          unlistenEvents = null
          return
        }
        unlistenEvents = unlisten
      })
      .catch(() => {
        unlistenEvents = null
      })
  }

  if (!snapshotState && !loadPromise) {
    loadPromise = readerBridge
      .readerGetSettings()
      .then((loaded) => {
        snapshotState = mergeIncomingSnapshot(loaded)
        notifyListeners()
        return snapshotState
      })
      .finally(() => {
        loadPromise = null
      })
  }
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener)
  ensureSubscribedAndLoaded()
  return () => {
    listeners.delete(listener)
  }
}

function getSnapshot(): ReaderSettingsSnapshot | null {
  return snapshotState
}

async function refreshAuthoritativeSnapshot(): Promise<void> {
  const startFontsRev = customFontsRevision
  const startSettledRev = fontMutationSettledRevision
  try {
    const authoritative = await readerBridge.readerGetSettings()
    if (
      startFontsRev !== customFontsRevision ||
      startSettledRev !== fontMutationSettledRevision
    ) {
      return
    }
    snapshotState = mergeIncomingSnapshot(authoritative)
    notifyListeners()
  } catch {
    // Ignore secondary refresh error.
  }
}

async function commitGlobalSettings(
  profile: ReaderProfile,
  revision: number
): Promise<void> {
  const startFontsRev = customFontsRevision
  try {
    const saved = await readerBridge.readerUpdateGlobalSettings(profile)
    if (globalWriteRevision === revision) {
      pendingGlobalProfile = null
      if (startFontsRev === customFontsRevision) {
        snapshotState = mergeIncomingSnapshot(saved)
        notifyListeners()
      } else {
        void refreshAuthoritativeSnapshot()
      }
    }
  } catch {
    if (globalWriteRevision === revision) {
      pendingGlobalProfile = null
      void refreshAuthoritativeSnapshot()
    }
  }
}

async function commitBookOverride(
  bookHash: string,
  patch: ReaderProfilePatch | null,
  revision: number
): Promise<void> {
  const startFontsRev = customFontsRevision
  try {
    const saved =
      patch === null
        ? await readerBridge.readerResetBookOverride(bookHash)
        : await readerBridge.readerSetBookOverride(bookHash, patch)
    if (bookWriteRevision.get(bookHash) === revision) {
      pendingBookPatches.delete(bookHash)
      if (startFontsRev === customFontsRevision) {
        snapshotState = mergeIncomingSnapshot(saved)
        notifyListeners()
      } else {
        void refreshAuthoritativeSnapshot()
      }
    }
  } catch {
    if (bookWriteRevision.get(bookHash) === revision) {
      pendingBookPatches.delete(bookHash)
      void refreshAuthoritativeSnapshot()
    }
  }
}

export function getReaderSettingsSnapshot(
  isMobile?: boolean
): ReaderSettingsSnapshot {
  return snapshotState ?? getFallbackSnapshot(isMobile)
}

export async function ensureReaderSettingsLoaded(): Promise<ReaderSettingsSnapshot> {
  ensureSubscribedAndLoaded()
  if (snapshotState) return snapshotState
  if (loadPromise) return loadPromise
  return getFallbackSnapshot()
}

export function updateGlobalReaderProfile(
  updater: ReaderProfile | ((prev: ReaderProfile) => ReaderProfile)
): void {
  const base = getReaderSettingsSnapshot()
  const nextProfile =
    typeof updater === "function" ? updater(base.global) : updater

  snapshotState = {
    ...base,
    global: nextProfile,
  }
  pendingGlobalProfile = nextProfile
  const rev = ++globalWriteRevision
  notifyListeners()

  if (globalFlushTimer !== null) {
    clearTimeout(globalFlushTimer)
  }
  globalFlushTimer = setTimeout(() => {
    globalFlushTimer = null
    const toSave = pendingGlobalProfile
    if (toSave) {
      void commitGlobalSettings(toSave, rev)
    }
  }, WRITE_DEBOUNCE_MS)
}

export function updateGlobalReaderField<
  G extends keyof ReaderProfile,
  K extends keyof ReaderProfile[G],
>(group: G, key: K, value: ReaderProfile[G][K]): void {
  updateGlobalReaderProfile((prev) => ({
    ...prev,
    [group]: {
      ...prev[group],
      [key]: value,
    },
  }))
}

export async function resetGlobalReaderSettings(
  isMobile?: boolean | undefined
): Promise<void> {
  if (globalFlushTimer !== null) {
    clearTimeout(globalFlushTimer)
    globalFlushTimer = null
  }
  const resolvedIsMobile = resolveDefaultIsMobile(isMobile)
  const defaults = getDefaultReaderProfile(resolvedIsMobile)
  const base = getReaderSettingsSnapshot(resolvedIsMobile)
  snapshotState = {
    ...base,
    global: defaults,
  }
  pendingGlobalProfile = defaults
  const rev = ++globalWriteRevision
  const startFontsRev = customFontsRevision
  notifyListeners()

  try {
    const saved = await readerBridge.readerUpdateGlobalSettings(defaults)
    if (globalWriteRevision === rev) {
      pendingGlobalProfile = null
      if (startFontsRev === customFontsRevision) {
        snapshotState = mergeIncomingSnapshot(saved)
        notifyListeners()
      } else {
        void refreshAuthoritativeSnapshot()
      }
    }
  } catch (error) {
    if (globalWriteRevision === rev) {
      pendingGlobalProfile = null
      void refreshAuthoritativeSnapshot()
    }
    throw error
  }
}

export function setBookReaderOverrideField<
  G extends keyof ReaderProfile,
  K extends keyof ReaderProfile[G],
>(bookHash: string, group: G, key: K, value: ReaderProfile[G][K]): void {
  const normalizedHash = normalizeBookHashKey(bookHash)
  const base = getReaderSettingsSnapshot()
  const currentPatch =
    base.bookOverrides[normalizedHash] ?? base.bookOverrides[bookHash]
  const nextPatch = mergeBookPatchField(currentPatch, group, key, value)

  snapshotState = {
    ...base,
    bookOverrides: {
      ...base.bookOverrides,
      [normalizedHash]: nextPatch,
    },
  }
  pendingBookPatches.set(normalizedHash, nextPatch)
  const rev = (bookWriteRevision.get(normalizedHash) ?? 0) + 1
  bookWriteRevision.set(normalizedHash, rev)
  notifyListeners()

  const existingTimer = bookFlushTimers.get(normalizedHash)
  if (existingTimer !== undefined) {
    clearTimeout(existingTimer)
  }
  const timer = setTimeout(() => {
    bookFlushTimers.delete(normalizedHash)
    const toSave = pendingBookPatches.get(normalizedHash)
    if (toSave !== undefined) {
      void commitBookOverride(normalizedHash, toSave, rev)
    }
  }, WRITE_DEBOUNCE_MS)
  bookFlushTimers.set(normalizedHash, timer)
}

export function clearBookReaderOverrideField<
  G extends keyof ReaderProfile,
  K extends keyof ReaderProfile[G],
>(bookHash: string, group: G, key: K): void {
  const normalizedHash = normalizeBookHashKey(bookHash)
  const base = getReaderSettingsSnapshot()
  const currentPatch =
    base.bookOverrides[normalizedHash] ?? base.bookOverrides[bookHash]
  const nextPatch = clearBookPatchField(currentPatch, group, key)

  const nextOverrides = { ...base.bookOverrides }
  if (nextPatch === null) {
    delete nextOverrides[normalizedHash]
    delete nextOverrides[bookHash]
  } else {
    nextOverrides[normalizedHash] = nextPatch
  }

  snapshotState = {
    ...base,
    bookOverrides: nextOverrides,
  }
  pendingBookPatches.set(normalizedHash, nextPatch)
  const rev = (bookWriteRevision.get(normalizedHash) ?? 0) + 1
  bookWriteRevision.set(normalizedHash, rev)
  notifyListeners()

  const existingTimer = bookFlushTimers.get(normalizedHash)
  if (existingTimer !== undefined) {
    clearTimeout(existingTimer)
  }
  const timer = setTimeout(() => {
    bookFlushTimers.delete(normalizedHash)
    const toSave = pendingBookPatches.get(normalizedHash)
    if (toSave !== undefined) {
      void commitBookOverride(normalizedHash, toSave, rev)
    }
  }, WRITE_DEBOUNCE_MS)
  bookFlushTimers.set(normalizedHash, timer)
}

export async function resetBookReaderOverrides(bookHash: string): Promise<void> {
  const normalizedHash = normalizeBookHashKey(bookHash)
  const existingTimer = bookFlushTimers.get(normalizedHash)
  if (existingTimer !== undefined) {
    clearTimeout(existingTimer)
    bookFlushTimers.delete(normalizedHash)
  }
  pendingBookPatches.set(normalizedHash, null)
  const rev = (bookWriteRevision.get(normalizedHash) ?? 0) + 1
  bookWriteRevision.set(normalizedHash, rev)
  const startFontsRev = customFontsRevision

  const base = getReaderSettingsSnapshot()
  if (
    Object.hasOwn(base.bookOverrides, normalizedHash) ||
    Object.hasOwn(base.bookOverrides, bookHash)
  ) {
    const nextOverrides = { ...base.bookOverrides }
    delete nextOverrides[normalizedHash]
    delete nextOverrides[bookHash]
    snapshotState = {
      ...base,
      bookOverrides: nextOverrides,
    }
    notifyListeners()
  }

  try {
    const saved = await readerBridge.readerResetBookOverride(normalizedHash)
    if (bookWriteRevision.get(normalizedHash) === rev) {
      pendingBookPatches.delete(normalizedHash)
      if (startFontsRev === customFontsRevision) {
        snapshotState = mergeIncomingSnapshot(saved)
        notifyListeners()
      } else {
        void refreshAuthoritativeSnapshot()
      }
    }
  } catch (error) {
    if (bookWriteRevision.get(normalizedHash) === rev) {
      pendingBookPatches.delete(normalizedHash)
      void refreshAuthoritativeSnapshot()
    }
    throw error
  }
}

export async function setBookReaderPatch(
  bookHash: string,
  patch: ReaderProfilePatch
): Promise<void> {
  const normalizedHash = normalizeBookHashKey(bookHash)
  const pruned = pruneEmptyPatch(patch)
  if (pruned === null) {
    await resetBookReaderOverrides(normalizedHash)
    return
  }

  const base = getReaderSettingsSnapshot()
  snapshotState = {
    ...base,
    bookOverrides: {
      ...base.bookOverrides,
      [normalizedHash]: pruned,
    },
  }
  pendingBookPatches.set(normalizedHash, pruned)
  const rev = (bookWriteRevision.get(normalizedHash) ?? 0) + 1
  bookWriteRevision.set(normalizedHash, rev)
  notifyListeners()

  const existingTimer = bookFlushTimers.get(normalizedHash)
  if (existingTimer !== undefined) {
    clearTimeout(existingTimer)
    bookFlushTimers.delete(normalizedHash)
  }
  await commitBookOverride(normalizedHash, pruned, rev)
}

export async function pickAndImportCustomFont(): Promise<ReaderSettingsSnapshot> {
  const myRev = ++customFontsRevision
  const updated = await readerBridge.readerPickAndImportFont()
  fontMutationSettledRevision += 1
  if (customFontsRevision === myRev) {
    snapshotState = mergeIncomingSnapshot(updated)
    notifyListeners()
    return snapshotState
  }
  void refreshAuthoritativeSnapshot()
  return snapshotState ?? mergeIncomingSnapshot(updated)
}

export async function deleteCustomFont(
  fontId: string
): Promise<ReaderSettingsSnapshot> {
  const myRev = ++customFontsRevision
  const updated = await readerBridge.readerDeleteCustomFont(fontId)
  evictSharedCustomFontBuffer(fontId)
  fontMutationSettledRevision += 1
  if (customFontsRevision === myRev) {
    snapshotState = mergeIncomingSnapshot(updated)
    notifyListeners()
    return snapshotState
  }
  void refreshAuthoritativeSnapshot()
  return snapshotState ?? mergeIncomingSnapshot(updated)
}

export async function loadSystemFonts(): Promise<readonly SystemFontFamily[]> {
  if (systemFontsState !== null) {
    return systemFontsState
  }
  if (!systemFontsPromise) {
    systemFontsPromise = readerBridge
      .readerListSystemFonts()
      .then((fonts) => {
        systemFontsState = fonts
        notifyListeners()
        return fonts
      })
      .catch(() => {
        systemFontsState = []
        notifyListeners()
        return []
      })
      .finally(() => {
        systemFontsPromise = null
      })
  }
  return systemFontsPromise
}

function rememberMobileHint(isMobile: boolean): void {
  if (isMobile) {
    inferredIsMobile = true
  }
}

export function useReaderSettingsSnapshot(
  isMobile = false
): ReaderSettingsSnapshot {
  useEffect(() => {
    rememberMobileHint(isMobile)
  }, [isMobile])
  const raw = useSyncExternalStore(
    subscribe,
    getSnapshot,
    () => getFallbackSnapshot(isMobile)
  )
  return raw ?? getFallbackSnapshot(isMobile)
}

export function useEffectiveReaderProfile(
  bookHash: string,
  isMobile = false
): {
  readonly snapshot: ReaderSettingsSnapshot
  readonly bookPatch: ReaderProfilePatch | undefined
  readonly effectiveProfile: ReaderProfile
} {
  const snapshot = useReaderSettingsSnapshot(isMobile)
  const normalizedHash = normalizeBookHashKey(bookHash)
  const bookPatch =
    snapshot.bookOverrides[normalizedHash] ?? snapshot.bookOverrides[bookHash]
  const effectiveProfile = useMemo(
    () => resolveEffectiveProfile(snapshot.global, bookPatch),
    [snapshot.global, bookPatch]
  )
  return {
    snapshot,
    bookPatch,
    effectiveProfile,
  }
}

export function useSystemFonts(enabled: boolean): {
  readonly systemFonts: readonly SystemFontFamily[]
  readonly isLoading: boolean
} {
  const snapshot = useSyncExternalStore(
    subscribe,
    () => systemFontsState,
    () => null
  )

  useEffect(() => {
    if (!enabled || systemFontsState !== null) return
    void loadSystemFonts()
  }, [enabled])

  return {
    systemFonts: snapshot ?? [],
    isLoading: enabled && snapshot === null,
  }
}

export async function flushPendingReaderSettingsWrites(): Promise<void> {
  const tasks: Array<Promise<void>> = []
  if (globalFlushTimer !== null) {
    clearTimeout(globalFlushTimer)
    globalFlushTimer = null
    if (pendingGlobalProfile) {
      tasks.push(commitGlobalSettings(pendingGlobalProfile, globalWriteRevision))
    }
  }
  for (const [bookHash, timer] of [...bookFlushTimers.entries()]) {
    clearTimeout(timer)
    bookFlushTimers.delete(bookHash)
    const patch = pendingBookPatches.get(bookHash)
    if (patch !== undefined) {
      const rev = bookWriteRevision.get(bookHash) ?? 0
      tasks.push(commitBookOverride(bookHash, patch, rev))
    }
  }
  await Promise.all(tasks)
}

export function resetReaderSettingsStoreForTest(): void {
  if (globalFlushTimer !== null) {
    clearTimeout(globalFlushTimer)
    globalFlushTimer = null
  }
  for (const timer of bookFlushTimers.values()) {
    clearTimeout(timer)
  }
  bookFlushTimers.clear()
  pendingGlobalProfile = null
  pendingBookPatches.clear()
  globalWriteRevision = 0
  customFontsRevision = 0
  fontMutationSettledRevision = 0
  bookWriteRevision.clear()
  unlistenEvents?.()
  unlistenEvents = null
  snapshotState = null
  systemFontsState = null
  loadPromise = null
  systemFontsPromise = null
  inferredIsMobile = null
  listeners.clear()
  sharedFontBuffersByBridge.get(readerBridge)?.clear()
}
