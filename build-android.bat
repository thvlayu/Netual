@echo off
echo Building Netual Android APK...
cd android
call gradlew.bat assembleDebug
echo.
echo Build complete!
echo APK location: app\build\outputs\apk\debug\app-debug.apk
pause
