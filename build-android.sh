#!/bin/bash
echo "Building Netual Android APK..."
cd android

# Use gradle directly instead of wrapper
if ! command -v gradle &> /dev/null; then
    echo "Error: Gradle not found in PATH"
    echo "Please install Gradle 8.4+ or Android Studio"
    exit 1
fi

gradle assembleDebug
echo ""
echo "Build complete!"
echo "APK location: app/build/outputs/apk/debug/app-debug.apk"
