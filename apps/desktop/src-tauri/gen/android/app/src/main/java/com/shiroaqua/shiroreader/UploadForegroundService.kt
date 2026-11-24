package com.shiroaqua.shiroreader

import android.app.Notification
import android.app.Service
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationManagerCompat
import androidx.core.app.ServiceCompat

class UploadForegroundService : Service() {
  companion object {
    @Volatile
    var currentInstance: UploadForegroundService? = null
  }

  override fun onCreate() {
    super.onCreate()
    currentInstance = this
  }

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    val initialNotification = UploadNotificationCoordinator.buildForegroundStartNotification(this)
    val fgsType = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
      ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC
    } else {
      0
    }
    ServiceCompat.startForeground(
      this,
      UploadNotificationCoordinator.UPLOAD_NOTIFICATION_ID,
      initialNotification,
      fgsType,
    )
    UploadNotificationCoordinator.reconcileServiceStarted(this)
    return START_NOT_STICKY
  }

  fun transitionToCompleted(notification: Notification) {
    ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_DETACH)
    if (UploadNotificationCoordinator.hasNotificationPermission(this)) {
      try {
        NotificationManagerCompat.from(this)
          .notify(UploadNotificationCoordinator.UPLOAD_NOTIFICATION_ID, notification)
      } catch (_: SecurityException) {
      }
    }
    stopSelf()
  }

  fun stopAndRemoveNotification() {
    ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_REMOVE)
    stopSelf()
  }

  override fun onTimeout(startId: Int, fgsType: Int) {
    ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_DETACH)
    stopSelf(startId)
  }

  override fun onDestroy() {
    if (currentInstance === this) {
      currentInstance = null
    }
    super.onDestroy()
  }

  override fun onBind(intent: Intent?): IBinder? = null
}
