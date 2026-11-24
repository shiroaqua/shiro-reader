package com.shiroaqua.shiroreader

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent

class UploadNotificationDismissReceiver : BroadcastReceiver() {
  override fun onReceive(context: Context, intent: Intent) {
    val generation = intent.getLongExtra(UploadNotificationCoordinator.EXTRA_GENERATION, -1L)
    UploadNotificationCoordinator.onNotificationDismissed(context.applicationContext, generation)
  }
}
