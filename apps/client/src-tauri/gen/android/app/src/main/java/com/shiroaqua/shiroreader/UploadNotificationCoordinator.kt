package com.shiroaqua.shiroreader

import android.Manifest
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat

object UploadNotificationCoordinator {
  const val UPLOAD_NOTIFICATION_ID = 41001
  const val CHANNEL_PROGRESS = "shiro_upload_progress"
  const val CHANNEL_COMPLETE = "shiro_upload_complete"
  const val ACTION_OPEN_UPLOAD_UI = "com.shiroaqua.shiroreader.ACTION_OPEN_UPLOAD_UI"
  const val EXTRA_IS_COMPLETED = "is_completed"
  const val EXTRA_GENERATION = "generation"

  private const val REQUEST_CODE_OPEN_UI = 100
  private const val REQUEST_CODE_DISMISS = 101

  sealed interface State {
    data object Idle : State

    data class Uploading(
      val title: String,
      val body: String,
      val progress: Int,
      val max: Int,
      val indeterminate: Boolean,
    ) : State

    data class Completed(
      val title: String,
      val body: String,
    ) : State
  }

  private val lock = Any()
  private var state: State = State.Idle
  private var generation: Long = 0L

  fun ensureChannels(context: Context) {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
    val notificationManager = context.getSystemService(NotificationManager::class.java) ?: return

    val progressChannel = NotificationChannel(
      CHANNEL_PROGRESS,
      context.getString(R.string.upload_channel_progress_name),
      NotificationManager.IMPORTANCE_LOW,
    ).apply {
      description = context.getString(R.string.upload_channel_progress_desc)
      setSound(null, null)
      enableVibration(false)
      enableLights(false)
      setShowBadge(false)
    }

    val completeChannel = NotificationChannel(
      CHANNEL_COMPLETE,
      context.getString(R.string.upload_channel_complete_name),
      NotificationManager.IMPORTANCE_DEFAULT,
    ).apply {
      description = context.getString(R.string.upload_channel_complete_desc)
    }

    notificationManager.createNotificationChannel(progressChannel)
    notificationManager.createNotificationChannel(completeChannel)
  }

  fun hasBaseNotificationPermission(context: Context): Boolean {
    if (!NotificationManagerCompat.from(context).areNotificationsEnabled()) {
      return false
    }
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
      val granted = ContextCompat.checkSelfPermission(
        context,
        Manifest.permission.POST_NOTIFICATIONS,
      ) == PackageManager.PERMISSION_GRANTED
      if (!granted) return false
    }
    return true
  }

  fun hasNotificationPermission(context: Context): Boolean {
    ensureChannels(context)
    if (!hasBaseNotificationPermission(context)) {
      return false
    }
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
      val notificationManager = context.getSystemService(NotificationManager::class.java)
      val progressChannel = notificationManager?.getNotificationChannel(CHANNEL_PROGRESS)
      if (progressChannel != null && progressChannel.importance == NotificationManager.IMPORTANCE_NONE) {
        return false
      }
      val completeChannel = notificationManager?.getNotificationChannel(CHANNEL_COMPLETE)
      if (completeChannel != null && completeChannel.importance == NotificationManager.IMPORTANCE_NONE) {
        return false
      }
    }
    return true
  }

  fun buildForegroundStartNotification(context: Context): Notification {
    ensureChannels(context)
    val snapshot = synchronized(lock) {
      val current = state
      val gen = generation
      if (current is State.Uploading) {
        current to gen
      } else {
        State.Uploading(
          title = context.getString(R.string.upload_notification_default_title),
          body = "",
          progress = 0,
          max = 100,
          indeterminate = true,
        ) to gen
      }
    }
    return buildOngoingNotification(context, snapshot.first, snapshot.second)
  }

  fun showOngoing(
    context: Context,
    title: String,
    body: String,
    progress: Int,
    max: Int,
    indeterminate: Boolean,
  ) {
    ensureChannels(context)
    val (uploading, gen) = synchronized(lock) {
      if (state !is State.Uploading) {
        generation += 1L
      }
      val next = State.Uploading(
        title = title,
        body = body,
        progress = progress.coerceIn(0, max.coerceAtLeast(1)),
        max = max.coerceAtLeast(1),
        indeterminate = indeterminate,
      )
      state = next
      next to generation
    }

    val notification = buildOngoingNotification(context, uploading, gen)
    val service = UploadForegroundService.currentInstance
    if (service != null) {
      notifySafe(context, notification)
      return
    }

    try {
      ContextCompat.startForegroundService(
        context,
        Intent(context, UploadForegroundService::class.java),
      )
    } catch (_: Exception) {
      notifySafe(context, notification)
    }
  }

  fun showCompleted(context: Context, title: String, body: String) {
    ensureChannels(context)
    val (completed, gen) = synchronized(lock) {
      if (state !is State.Completed) {
        generation += 1L
      }
      val next = State.Completed(title = title, body = body)
      state = next
      next to generation
    }

    val notification = buildCompletedNotification(context, completed, gen)
    val service = UploadForegroundService.currentInstance
    if (service != null) {
      service.transitionToCompleted(notification)
    } else {
      notifySafe(context, notification)
    }
  }

  fun clear(context: Context) {
    synchronized(lock) {
      state = State.Idle
      generation += 1L
    }
    UploadForegroundService.currentInstance?.stopAndRemoveNotification()
    NotificationManagerCompat.from(context).cancel(UPLOAD_NOTIFICATION_ID)
  }

  fun reconcileServiceStarted(service: UploadForegroundService) {
    var ongoingNotification: Notification? = null
    var completedNotification: Notification? = null
    var shouldStop = false

    synchronized(lock) {
      when (val current = state) {
        is State.Uploading -> {
          ongoingNotification = buildOngoingNotification(service, current, generation)
        }
        is State.Completed -> {
          completedNotification = buildCompletedNotification(service, current, generation)
        }
        State.Idle -> {
          shouldStop = true
        }
      }
    }

    when {
      ongoingNotification != null -> notifySafe(service, ongoingNotification!!)
      completedNotification != null -> service.transitionToCompleted(completedNotification!!)
      shouldStop -> service.stopAndRemoveNotification()
    }
  }

  fun onNotificationTapped(isCompleted: Boolean) {
    synchronized(lock) {
      if (isCompleted && state is State.Completed) {
        state = State.Idle
        generation += 1L
      }
    }
  }

  fun onNotificationDismissed(context: Context, intentGeneration: Long) {
    var repostNotification: Notification? = null
    var notifyBridgeDismissed = false

    synchronized(lock) {
      if (intentGeneration != generation) {
        return
      }
      when (val current = state) {
        is State.Uploading -> {
          repostNotification = buildOngoingNotification(context, current, generation)
        }
        is State.Completed -> {
          state = State.Idle
          generation += 1L
          notifyBridgeDismissed = true
        }
        State.Idle -> {}
      }
    }

    repostNotification?.let { notification ->
      if (hasBaseNotificationPermission(context)) {
        notifySafe(context, notification)
      }
    }

    if (notifyBridgeDismissed) {
      UploadNotificationBridge.notifyDismissed()
    }
  }

  private fun notifySafe(context: Context, notification: Notification) {
    if (!hasBaseNotificationPermission(context)) return
    try {
      NotificationManagerCompat.from(context).notify(UPLOAD_NOTIFICATION_ID, notification)
    } catch (_: SecurityException) {
    }
  }

  private fun openUiPendingIntent(context: Context, isCompleted: Boolean): PendingIntent {
    val intent = Intent(context, MainActivity::class.java).apply {
      action = ACTION_OPEN_UPLOAD_UI
      flags = Intent.FLAG_ACTIVITY_SINGLE_TOP or
        Intent.FLAG_ACTIVITY_CLEAR_TOP or
        Intent.FLAG_ACTIVITY_NEW_TASK
      putExtra(EXTRA_IS_COMPLETED, isCompleted)
    }
    return PendingIntent.getActivity(
      context,
      REQUEST_CODE_OPEN_UI,
      intent,
      PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
    )
  }

  private fun dismissPendingIntent(context: Context, gen: Long): PendingIntent {
    val intent = Intent(context, UploadNotificationDismissReceiver::class.java).apply {
      putExtra(EXTRA_GENERATION, gen)
    }
    return PendingIntent.getBroadcast(
      context,
      REQUEST_CODE_DISMISS,
      intent,
      PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
    )
  }

  private fun buildOngoingNotification(
    context: Context,
    uploading: State.Uploading,
    gen: Long,
  ): Notification {
    val builder = NotificationCompat.Builder(context, CHANNEL_PROGRESS)
      .setSmallIcon(R.drawable.ic_stat_upload)
      .setContentTitle(uploading.title)
      .setContentText(uploading.body)
      .setOngoing(true)
      .setAutoCancel(false)
      .setOnlyAlertOnce(true)
      .setSilent(true)
      .setProgress(uploading.max, uploading.progress, uploading.indeterminate)
      .setCategory(NotificationCompat.CATEGORY_PROGRESS)
      .setPriority(NotificationCompat.PRIORITY_LOW)
      .setVisibility(NotificationCompat.VISIBILITY_PUBLIC)
      .setContentIntent(openUiPendingIntent(context, isCompleted = false))
      .setDeleteIntent(dismissPendingIntent(context, gen))

    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
      builder.setForegroundServiceBehavior(NotificationCompat.FOREGROUND_SERVICE_IMMEDIATE)
    }
    return builder.build()
  }

  private fun buildCompletedNotification(
    context: Context,
    completed: State.Completed,
    gen: Long,
  ): Notification {
    return NotificationCompat.Builder(context, CHANNEL_COMPLETE)
      .setSmallIcon(R.drawable.ic_stat_upload_done)
      .setContentTitle(completed.title)
      .setContentText(completed.body)
      .setOngoing(false)
      .setAutoCancel(true)
      .setOnlyAlertOnce(true)
      .setProgress(0, 0, false)
      .setPriority(NotificationCompat.PRIORITY_DEFAULT)
      .setVisibility(NotificationCompat.VISIBILITY_PUBLIC)
      .setContentIntent(openUiPendingIntent(context, isCompleted = true))
      .setDeleteIntent(dismissPendingIntent(context, gen))
      .build()
  }
}
