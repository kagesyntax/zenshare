#!/usr/bin/env bash
#
# Build the Android APK.
#
# Shared by the standalone Android workflow (fast feedback on every push) and the
# release workflow (attaches the APK to a tag). One script so the two cannot
# drift apart — a release that packages differently from CI is a release nobody
# has tested.
#
# Usage: build-apk.sh <version>      e.g. build-apk.sh 0.2.0
#
# Requires: cargo-ndk, an Android SDK with build-tools and a platform, and the
# aarch64-linux-android / x86_64-linux-android Rust targets.
set -euo pipefail

VERSION="${1:?usage: build-apk.sh <version>}"

SDK="${ANDROID_HOME:-/usr/local/lib/android/sdk}"
BUILD_TOOLS="$SDK/build-tools/$(ls "$SDK/build-tools" | sort -V | tail -1)"
PLATFORM_JAR="$SDK/platforms/$(ls "$SDK/platforms" | sort -V | tail -1)/android.jar"

echo "version:     $VERSION"
echo "build-tools: $BUILD_TOOLS"
echo "platform:    $PLATFORM_JAR"

# arm64-v8a covers every phone sold since roughly 2018. x86_64 exists so the
# emulator works. There is no point building 32-bit ARM for a v0.1.
cargo ndk -t arm64-v8a -t x86_64 -o jniLibs build --release -p zs-android

mkdir -p dist/apk
"$BUILD_TOOLS/aapt2" link \
  -o dist/apk/base.apk \
  -I "$PLATFORM_JAR" \
  --manifest crates/zs-android/android/AndroidManifest.xml \
  --version-code 1 \
  --version-name "$VERSION" \
  --min-sdk-version 26 \
  --target-sdk-version 34

# Native libraries are not resources, so they go in after aapt2 has produced the
# APK skeleton.
pushd dist/apk > /dev/null
mkdir -p lib/arm64-v8a lib/x86_64
cp ../../jniLibs/arm64-v8a/libzenshare.so lib/arm64-v8a/
cp ../../jniLibs/x86_64/libzenshare.so lib/x86_64/
zip -q -r base.apk lib
popd > /dev/null

"$BUILD_TOOLS/zipalign" -f 4 dist/apk/base.apk dist/apk/aligned.apk

# A throwaway key, regenerated every build. These are not Play Store releases and
# the certificate proves nothing about identity — only that the APK is internally
# consistent. Saying so here stops anyone mistaking it for a signing setup.
keytool -genkeypair -keystore dist/debug.keystore \
  -storepass android -keypass android -alias androiddebugkey \
  -dname "CN=Android Debug,O=Android,C=US" \
  -keyalg RSA -keysize 2048 -validity 10000

"$BUILD_TOOLS/apksigner" sign \
  --ks dist/debug.keystore --ks-pass pass:android --key-pass pass:android \
  --out "dist/zenshare-$VERSION-android.apk" \
  dist/apk/aligned.apk

"$BUILD_TOOLS/apksigner" verify --print-certs "dist/zenshare-$VERSION-android.apk" | head -5
ls -la dist/*.apk
