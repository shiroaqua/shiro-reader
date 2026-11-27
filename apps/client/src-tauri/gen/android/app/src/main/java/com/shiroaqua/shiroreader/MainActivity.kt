package com.shiroaqua.shiroreader

import android.Manifest
import android.content.Intent
import android.graphics.Color
import android.graphics.drawable.ColorDrawable
import android.graphics.drawable.Drawable
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.Looper
import android.view.ActionMode
import android.view.Menu
import android.view.MenuInflater
import android.view.View
import android.view.Window
import android.webkit.JavascriptInterface
import android.webkit.WebView
import android.widget.PopupMenu
import androidx.activity.SystemBarStyle
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import java.util.Locale
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean

class MainActivity : TauriActivity() {
  @Volatile
  private var suppressSelectionActionMenu: Boolean = false

  @Volatile
  private var immersiveReaderActive: Boolean = false

  @Volatile
  private var lastSafeTopDp: Float = 0f

  @Volatile
  private var lastSafeRightDp: Float = 0f

  @Volatile
  private var lastSafeBottomDp: Float = 0f

  @Volatile
  private var lastSafeLeftDp: Float = 0f

  private var savedAppStatusBarColor: Int? = null
  private var savedAppNavigationBarColor: Int? = null
  private var savedAppDecorBackground: Drawable? = null
  private var wasImmersiveUiApplied: Boolean = false

  private var webViewRef: WebView? = null

  inner class ShiroReaderAndroidBridge {
    @JavascriptInterface
    fun setSuppressSelectionMenu(suppress: Boolean) {
      suppressSelectionActionMenu = suppress
    }

    @JavascriptInterface
    fun getSafeAreaInsetsJson(): String {
      return String.format(
        Locale.US,
        "{\"top\":%.2f,\"right\":%.2f,\"bottom\":%.2f,\"left\":%.2f}",
        lastSafeTopDp,
        lastSafeRightDp,
        lastSafeBottomDp,
        lastSafeLeftDp
      )
    }

    @JavascriptInterface
    fun setReaderSystemUi(
      immersive: Boolean,
      hideSystemBars: Boolean,
      isDarkSurface: Boolean,
      backgroundColorHex: String
    ) {
      runOnUiThread {
        immersiveReaderActive = immersive
        val content = findViewById<View>(android.R.id.content) ?: return@runOnUiThread
        val controller = WindowCompat.getInsetsController(window, content)
        controller.systemBarsBehavior =
          WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
        if (immersive) {
          if (!wasImmersiveUiApplied) {
            @Suppress("DEPRECATION")
            savedAppStatusBarColor = window.statusBarColor
            @Suppress("DEPRECATION")
            savedAppNavigationBarColor = window.navigationBarColor
            savedAppDecorBackground = window.decorView.background
            wasImmersiveUiApplied = true
          }
          @Suppress("DEPRECATION")
          window.statusBarColor = Color.TRANSPARENT
          @Suppress("DEPRECATION")
          window.navigationBarColor = Color.TRANSPARENT
          controller.isAppearanceLightStatusBars = !isDarkSurface
          controller.isAppearanceLightNavigationBars = !isDarkSurface
          try {
            window.decorView.background = ColorDrawable(Color.parseColor(backgroundColorHex))
          } catch (_: IllegalArgumentException) {
            // Keep existing background color if input is malformed.
          }
          if (hideSystemBars) {
            controller.hide(WindowInsetsCompat.Type.systemBars())
          } else {
            controller.show(WindowInsetsCompat.Type.systemBars())
          }
          injectCachedSafeAreaCssVars()
        } else {
          if (wasImmersiveUiApplied) {
            @Suppress("DEPRECATION")
            savedAppStatusBarColor?.let { window.statusBarColor = it }
            @Suppress("DEPRECATION")
            savedAppNavigationBarColor?.let { window.navigationBarColor = it }
            window.decorView.background = savedAppDecorBackground
            savedAppStatusBarColor = null
            savedAppNavigationBarColor = null
            savedAppDecorBackground = null
            wasImmersiveUiApplied = false
          }
          controller.isAppearanceLightStatusBars = true
          controller.isAppearanceLightNavigationBars = true
          controller.show(WindowInsetsCompat.Type.systemBars())
        }
        ViewCompat.requestApplyInsets(content)
      }
    }

    private fun isBidiControl(ch: Char): Boolean {
      return ch == '\u061C' ||
        ch == '\u200E' ||
        ch == '\u200F' ||
        (ch in '\u202A'..'\u202E') ||
        (ch in '\u2066'..'\u2069')
    }

    @JavascriptInterface
    fun openExternalUrl(rawUrl: String): Boolean {
      val trimmed = rawUrl.trim()
      if (trimmed.isEmpty() || trimmed.length > 4096) {
        return false
      }
      if (
        trimmed.any {
          it.isISOControl() ||
            it.isWhitespace() ||
            isBidiControl(it) ||
            it == '"' ||
            it == '<' ||
            it == '>' ||
            it == '`' ||
            it == '\\'
        }
      ) {
        return false
      }
      val uri = Uri.parse(trimmed)
      val scheme = uri.scheme?.lowercase(Locale.US) ?: return false
      if (scheme != "http" && scheme != "https" && scheme != "mailto") {
        return false
      }
      if ((scheme == "http" || scheme == "https") && uri.host.isNullOrBlank()) {
        return false
      }
      if (scheme == "mailto") {
        val ssp = uri.schemeSpecificPart
        val recipient = ssp?.substringBefore('?')?.trim().orEmpty()
        if (ssp.isNullOrBlank() || recipient.isEmpty()) {
          return false
        }
      }
      val intent = Intent(Intent.ACTION_VIEW, uri).apply {
        addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        if (scheme == "http" || scheme == "https") {
          addCategory(Intent.CATEGORY_BROWSABLE)
        }
      }
      if (Looper.myLooper() == Looper.getMainLooper()) {
        return try {
          startActivity(intent)
          true
        } catch (_: Exception) {
          false
        }
      }
      val launched = AtomicBoolean(false)
      val latch = CountDownLatch(1)
      runOnUiThread {
        try {
          startActivity(intent)
          launched.set(true)
        } catch (_: Exception) {
          launched.set(false)
        } finally {
          latch.countDown()
        }
      }
      return try {
        if (latch.await(2, TimeUnit.SECONDS)) launched.get() else false
      } catch (_: InterruptedException) {
        Thread.currentThread().interrupt()
        false
      }
    }
  }

  private inner class HeadlessFloatingActionMode(
    private val callback: ActionMode.Callback
  ) : ActionMode() {
    private val emptyMenu: Menu = PopupMenu(this@MainActivity, window.decorView).menu
    private var currentTitle: CharSequence? = null
    private var currentSubtitle: CharSequence? = null
    private var currentCustomView: View? = null
    private var isFinished: Boolean = false

    init {
      type = TYPE_FLOATING
    }

    override fun setTitle(title: CharSequence?) {
      currentTitle = title
    }

    override fun setTitle(resId: Int) {
      currentTitle = getString(resId)
    }

    override fun setSubtitle(subtitle: CharSequence?) {
      currentSubtitle = subtitle
    }

    override fun setSubtitle(resId: Int) {
      currentSubtitle = getString(resId)
    }

    override fun setCustomView(view: View?) {
      currentCustomView = view
    }

    override fun invalidate() {
      if (!isFinished) {
        callback.onPrepareActionMode(this, emptyMenu)
        emptyMenu.clear()
      }
    }

    override fun finish() {
      if (!isFinished) {
        isFinished = true
        callback.onDestroyActionMode(this)
      }
    }

    override fun getMenu(): Menu = emptyMenu

    override fun getTitle(): CharSequence? = currentTitle

    override fun getSubtitle(): CharSequence? = currentSubtitle

    override fun getCustomView(): View? = currentCustomView

    override fun getMenuInflater(): MenuInflater = MenuInflater(this@MainActivity)
  }

  private inner class SelectionSuppressWindowCallback(
    private val delegate: Window.Callback
  ) : Window.Callback by delegate {
    override fun onWindowStartingActionMode(callback: ActionMode.Callback?): ActionMode? {
      return delegate.onWindowStartingActionMode(callback)
    }

    override fun onWindowStartingActionMode(
      callback: ActionMode.Callback?,
      type: Int
    ): ActionMode? {
      if (
        suppressSelectionActionMenu &&
        type == ActionMode.TYPE_FLOATING &&
        callback != null
      ) {
        val mode = HeadlessFloatingActionMode(callback)
        if (callback.onCreateActionMode(mode, mode.menu)) {
          mode.menu.clear()
          callback.onPrepareActionMode(mode, mode.menu)
          mode.menu.clear()
          return mode
        }
        return null
      }
      return delegate.onWindowStartingActionMode(callback, type)
    }

    override fun onActionModeStarted(mode: ActionMode?) {
      if (suppressSelectionActionMenu && mode?.type == ActionMode.TYPE_FLOATING) {
        mode.menu?.clear()
      }
      delegate.onActionModeStarted(mode)
    }
  }

  override fun onCreate(savedInstanceState: Bundle?) {
    // The web UI is light-only: keep dark status/navigation bar icons even when
    // the system is in dark mode, otherwise they vanish on the white header.
    enableEdgeToEdge(
      statusBarStyle = SystemBarStyle.light(Color.TRANSPARENT, Color.TRANSPARENT),
      navigationBarStyle = SystemBarStyle.light(Color.TRANSPARENT, Color.TRANSPARENT),
    )
    super.onCreate(savedInstanceState)
    val existingCallback = window.callback
    if (existingCallback != null) {
      window.callback = SelectionSuppressWindowCallback(existingCallback)
    }
    UploadNotificationBridge.registerContext(applicationContext)
    UploadNotificationCoordinator.ensureChannels(this)
    handleUploadNotificationIntent(intent)

    // Keep the web content clear of the status bar, cutout, gesture/navigation
    // bar and the on-screen keyboard instead of relying on WebView safe-area
    // support, so dialogs and inputs are never hidden behind the keyboard.
    val content = findViewById<View>(android.R.id.content)
    ViewCompat.setOnApplyWindowInsetsListener(content) { view, insets ->
      injectSafeAreaCssVars(insets)
      val bars = insets.getInsets(
        WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout()
      )
      val ime = insets.getInsets(WindowInsetsCompat.Type.ime())
      if (immersiveReaderActive) {
        view.setPadding(0, 0, 0, ime.bottom)
      } else {
        view.setPadding(bars.left, bars.top, bars.right, maxOf(bars.bottom, ime.bottom))
      }
      WindowInsetsCompat.CONSUMED
    }
  }

  private fun dispatchSafeAreaCssVars(
    topDp: Float,
    rightDp: Float,
    bottomDp: Float,
    leftDp: Float
  ) {
    val webView = webViewRef ?: return
    val top = String.format(Locale.US, "%.2fpx", topDp)
    val right = String.format(Locale.US, "%.2fpx", rightDp)
    val bottom = String.format(Locale.US, "%.2fpx", bottomDp)
    val left = String.format(Locale.US, "%.2fpx", leftDp)
    val js = """
      (function(){
        var s = document.documentElement && document.documentElement.style;
        if (!s) return;
        s.setProperty('--shiro-safe-top', '$top');
        s.setProperty('--shiro-safe-right', '$right');
        s.setProperty('--shiro-safe-bottom', '$bottom');
        s.setProperty('--shiro-safe-left', '$left');
        window.dispatchEvent(new CustomEvent('shiro:safe-area-change'));
      })();
    """.trimIndent()
    webView.post {
      if (webView === webViewRef) {
        webView.evaluateJavascript(js, null)
      }
    }
  }

  private fun injectCachedSafeAreaCssVars() {
    dispatchSafeAreaCssVars(lastSafeTopDp, lastSafeRightDp, lastSafeBottomDp, lastSafeLeftDp)
  }

  private fun injectSafeAreaCssVars(insets: WindowInsetsCompat) {
    val stable = insets.getInsetsIgnoringVisibility(
      WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout()
    )
    val density = resources.displayMetrics.density.coerceAtLeast(1f)
    lastSafeTopDp = stable.top / density
    lastSafeRightDp = stable.right / density
    lastSafeBottomDp = stable.bottom / density
    lastSafeLeftDp = stable.left / density
    injectCachedSafeAreaCssVars()
  }

  override fun onNewIntent(intent: Intent) {
    super.onNewIntent(intent)
    setIntent(intent)
    handleUploadNotificationIntent(intent)
  }

  override fun onWebViewCreate(webView: WebView) {
    super.onWebViewCreate(webView)
    webViewRef = webView
    webView.settings.setSupportZoom(false)
    webView.settings.builtInZoomControls = false
    webView.settings.displayZoomControls = false
    webView.addJavascriptInterface(ShiroReaderAndroidBridge(), "__SHIRO_ANDROID_READER__")
    findViewById<View>(android.R.id.content)?.let { ViewCompat.requestApplyInsets(it) }
  }

  override fun onDestroy() {
    webViewRef = null
    super.onDestroy()
  }

  private fun handleUploadNotificationIntent(intent: Intent?) {
    if (intent?.action != UploadNotificationCoordinator.ACTION_OPEN_UPLOAD_UI) {
      return
    }
    val isCompleted = intent.getBooleanExtra(UploadNotificationCoordinator.EXTRA_IS_COMPLETED, false)
    intent.action = null
    UploadNotificationCoordinator.onNotificationTapped(isCompleted)
    UploadNotificationBridge.notifyTapped(isCompleted)
  }

  fun requestUploadNotificationPermission(requestId: Long) {
    if (UploadNotificationCoordinator.hasNotificationPermission(this)) {
      UploadNotificationBridge.notifyPermissionResult(requestId, true)
      return
    }
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) {
      UploadNotificationBridge.notifyPermissionResult(requestId, false)
      return
    }
    requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS)) { granted ->
      val allowed = granted == true && UploadNotificationCoordinator.hasNotificationPermission(this)
      UploadNotificationBridge.notifyPermissionResult(requestId, allowed)
    }
  }
}
