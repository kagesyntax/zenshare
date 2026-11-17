#!/usr/bin/env bash
#
# Launch the APK on a running emulator, then verify two things:
#
#   1. the process is still alive after 15 seconds (it used to crash at startup)
#   2. received files go somewhere a file manager can actually reach
#
# The second check exists because the first Android build wrote into
# `Android/data/<pkg>/`, which no file manager on the device can open. A
# transfer that "succeeds" into an invisible folder has not succeeded.
#
# Lives in a file because reactivecircus/android-emulator-runner executes its
# `script` input one line at a time, which breaks multi-line `if ... fi`.
set -uo pipefail

APK="${1:?usage: smoke-android.sh <apk>}"

echo "installing $APK"
adb install -r "$APK" || exit 1

adb logcat -c
adb shell am start -n dev.zenshare.app/android.app.NativeActivity
sleep 15

echo "===== zenshare log ====="
adb logcat -d -s zenshare:V | tail -80

echo "===== crash buffer ====="
adb logcat -d -b crash | tail -60

if ! adb shell pidof dev.zenshare.app > /dev/null 2>&1; then
  echo "::error::the app exited after launch"
  adb exec-out screencap -p > screen.png || true
  exit 1
fi
echo "OK: process alive"

# Pull the downloads path the app logged, and require it to be shared storage.
dir=$(adb logcat -d -s zenshare:V | grep -o 'downloads dir: .*' | tail -1 | sed 's/downloads dir: //' | tr -d '\r')
echo "downloads dir reported as: '$dir'"

if [ -z "$dir" ]; then
  echo "::error::the app did not report a downloads directory"
  exit 1
fi

case "$dir" in
  */Android/media/*) echo "OK: shared storage (Android/media)" ;;
  */Android/data/*)
    echo "::error::still writing to Android/data, which file managers cannot open"
    exit 1
    ;;
  *)
    echo "::warning::unexpected downloads location: $dir"
    ;;
esac

# The app creates this directory at startup, so its existing proves the write
# permission was real rather than assumed.
if adb shell ls -ld "$dir" > /dev/null 2>&1; then
  echo "OK: directory exists and is visible to the shell"
  adb shell ls -ld "$dir"
else
  echo "::error::directory does not exist: $dir"
  exit 1
fi

adb exec-out screencap -p > screen.png || true
echo "RESULT: pass"
