#!/usr/bin/env bash
#
# Launch the APK on a running emulator and report whether it survives.
#
# Called from android-smoke.yml. Lives in a file because
# reactivecircus/android-emulator-runner executes its `script` input one line at
# a time, which turns a multi-line `if ... fi` into a syntax error.
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

if adb shell pidof dev.zenshare.app > /dev/null 2>&1; then
  echo "RESULT: alive"
  adb exec-out screencap -p > screen.png || true
  exit 0
fi

echo "RESULT: dead — the process exited after launch"
adb exec-out screencap -p > screen.png || true
exit 1
