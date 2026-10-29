# Keep the UniFFI-generated JNA bindings intact - they are called by
# name from native code, so obfuscation/shrinking would silently break
# the bridge to refresh-mobile-ffi.
-keep class dev.refresh.ffi.** { *; }
-keep class com.sun.jna.** { *; }
-keepclassmembers class * implements com.sun.jna.Library { *; }
