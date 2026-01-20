#!/bin/bash
echo "Building Netual Android APK..."
cd android
./gradlew assembleDebug
echo ""
echo "Build complete!"
echo "APK location: app/build/outputs/apk/debug/app-debug.apk"
