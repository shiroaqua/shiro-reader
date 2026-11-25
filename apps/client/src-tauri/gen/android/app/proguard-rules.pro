# Add project specific ProGuard rules here.
# You can control the set of applied configuration files using the
# proguardFiles setting in build.gradle.
#
# For more details, see
#   http://developer.android.com/guide/developing/tools/proguard.html

# If your project uses WebView with JS, uncomment the following
# and specify the fully qualified class name to the JavaScript interface
# class:
#-keepclassmembers class fqcn.of.javascript.interface.for.webview {
#   public *;
#}

# Uncomment this to preserve the line number information for
# debugging stack traces.
#-keepattributes SourceFile,LineNumberTable

# If you keep the line number information, uncomment this to
# hide the original source file name.
#-renamesourcefileattribute SourceFile

-keepclassmembers class com.shiroaqua.shiroreader.MainActivity {
    public void requestUploadNotificationPermission(long);
}

-keepclasseswithmembers class com.shiroaqua.shiroreader.UploadNotificationBridge {
    public static native void nativeRegisterContext(android.content.Context);
    public static native void nativeOnPermissionResult(long, boolean);
    public static native void nativeOnNotificationTapped(boolean);
    public static native void nativeOnNotificationDismissed();
    public static void showOngoingFromNative(android.content.Context, java.lang.String, java.lang.String, int, int, boolean);
    public static void showCompletedFromNative(android.content.Context, java.lang.String, java.lang.String);
    public static void clearFromNative(android.content.Context);
}