package com.shiroaqua.shiroreader

import android.content.Context
import android.os.Handler
import android.os.Looper

object UploadNotificationBridge {
  private val mainHandler = Handler(Looper.getMainLooper())

  init {
    try {
      System.loadLibrary("shiro_reader_client_lib")
    } catch (_: UnsatisfiedLinkError) {
    }
  }

  @JvmStatic external fun nativeRegisterContext(context: Context)
  @JvmStatic external fun nativeOnPermissionResult(requestId: Long, granted: Boolean)
  @JvmStatic external fun nativeOnNotificationTapped(isCompleted: Boolean)
  @JvmStatic external fun nativeOnNotificationDismissed()

  fun registerContext(context: Context) {
    try {
      nativeRegisterContext(context.applicationContext)
    } catch (_: UnsatisfiedLinkError) {
    }
  }

  fun notifyPermissionResult(requestId: Long, granted: Boolean) {
    try {
      nativeOnPermissionResult(requestId, granted)
    } catch (_: UnsatisfiedLinkError) {
    }
  }

  fun notifyTapped(isCompleted: Boolean) {
    try {
      nativeOnNotificationTapped(isCompleted)
    } catch (_: UnsatisfiedLinkError) {
    }
  }

  fun notifyDismissed() {
    try {
      nativeOnNotificationDismissed()
    } catch (_: UnsatisfiedLinkError) {
    }
  }

  @JvmStatic
  fun showOngoingFromNative(
    context: Context,
    title: String,
    body: String,
    progress: Int,
    max: Int,
    indeterminate: Boolean,
  ) {
    val appContext = context.applicationContext
    mainHandler.post {
      UploadNotificationCoordinator.showOngoing(
        context = appContext,
        title = title,
        body = body,
        progress = progress,
        max = max,
        indeterminate = indeterminate,
      )
    }
  }

  @JvmStatic
  fun showCompletedFromNative(context: Context, title: String, body: String) {
    val appContext = context.applicationContext
    mainHandler.post {
      UploadNotificationCoordinator.showCompleted(
        context = appContext,
        title = title,
        body = body,
      )
    }
  }

  @JvmStatic
  fun clearFromNative(context: Context) {
    val appContext = context.applicationContext
    mainHandler.post {
      UploadNotificationCoordinator.clear(appContext)
    }
  }
}
