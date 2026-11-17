#!/usr/bin/env bash
#
# Launch the APK on a running emulator, then verify two things:
#
#   1. the process is still alive after 15 seconds (it used to crash at startup)
#   2. received files go somewhere a file manager can actually reach
#
# The second check is a filesystem assertion, not a log assertion: logcat was
# empty on one run and the test became meaningless. The app creates its download
# directory at startup, so the directory existing is proof that the write
# permission was real and that the location is shared storage.
#
# Lives in a file because reactivecircus/android-emulator-runner executes its
# `script` input one line at a time, which breaks multi-line `if ... fi`.
set -uo pipefail

APK="${1:?usage: smoke-android.sh <apk>}"

MEDIA_DIR="/storage/emulated/0/Android/media/dev.zenshare.app/ZenShare"
DATA_DIR="/storage/emulated/0/Android/data/dev.zenshare.app/files/ZenShare"

echo "installing $APK"
adb install -r "$APK" || exit 1

adb logcat -c || true
adb shell am start -n dev.zenshare.app/android.app.NativeActivity || exit 1
sleep 15

echo "===== process ====="
adb shell ps -A | grep -i zenshare || true

echo "===== zenshare log (best effort) ====="
adb logcat -d -s zenshare:V | tail -60 || true

echo "===== crash buffer (best effort) ====="
adb logcat -d -b crash | tail -40 || true

echo "===== shared storage ====="
adb shell ls -la /storage/emulated/0/Android/media/ 2>&1 | head -10 || true
adb shell ls -ld "$MEDIA_DIR" 2>&1 || true
adb shell ls -ld "$DATA_DIR" 2>&1 || true

adb exec-out screencap -p > screen.png || true

if ! adb shell pidof dev.zenshare.app > /dev/null 2>&1; then
  echo "::error::the app exited after launch"
  exit 1
fi
echo "OK: process alive"

if adb shell ls -d "$DATA_DIR" > /dev/null 2>&1; then
  echo "::error::still writing to Android/data, which no file manager can open"
  exit 1
fi

if ! adb shell ls -d "$MEDIA_DIR" > /dev/null 2>&1; then
  echo "::error::expected download directory missing: $MEDIA_DIR"
  exit 1
fi

echo "OK: downloads land in shared storage"
adb shell ls -ld "$MEDIA_DIR"
echo "RESULT: pass"
