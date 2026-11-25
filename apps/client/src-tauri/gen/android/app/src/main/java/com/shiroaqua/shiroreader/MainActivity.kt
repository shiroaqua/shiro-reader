package com.shiroaqua.shiroreader

import android.Manifest
import android.content.Intent
import android.graphics.Color
import android.os.Build
import android.os.Bundle
import android.view.View
import android.webkit.WebView
import androidx.activity.SystemBarStyle
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    // The web UI is light-only: keep dark status/navigation bar icons even when
    // the system is in dark mode, otherwise they vanish on the white header.
    enableEdgeToEdge(
      statusBarStyle = SystemBarStyle.light(Color.TRANSPARENT, Color.TRANSPARENT),
      navigationBarStyle = SystemBarStyle.light(Color.TRANSPARENT, Color.TRANSPARENT),
    )
    super.onCreate(savedInstanceState)
    UploadNotificationBridge.registerContext(applicationContext)
    UploadNotificationCoordinator.ensureChannels(this)
    handleUploadNotificationIntent(intent)

    // Keep the web content clear of the status bar, cutout, gesture/navigation
    // bar and the on-screen keyboard instead of relying on WebView safe-area
    // support, so dialogs and inputs are never hidden behind the keyboard.
    val content = findViewById<View>(android.R.id.content)
    ViewCompat.setOnApplyWindowInsetsListener(content) { view, insets ->
      val bars = insets.getInsets(
        WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout()
      )
      val ime = insets.getInsets(WindowInsetsCompat.Type.ime())
      view.setPadding(bars.left, bars.top, bars.right, maxOf(bars.bottom, ime.bottom))
      WindowInsetsCompat.CONSUMED
    }
  }

  override fun onNewIntent(intent: Intent) {
    super.onNewIntent(intent)
    setIntent(intent)
    handleUploadNotificationIntent(intent)
  }

  override fun onWebViewCreate(webView: WebView) {
    super.onWebViewCreate(webView)
    webView.settings.setSupportZoom(false)
    webView.settings.builtInZoomControls = false
    webView.settings.displayZoomControls = false
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
