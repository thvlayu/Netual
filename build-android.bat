@echo off
echo Building Netual Android APK...
cd android

REM Use gradle directly instead of wrapper
where gradle >nul 2>nul
if %ERRORLEVEL% NEQ 0 (
    echo Error: Gradle not found in PATH
    echo Please install Gradle 8.4+ or Android Studio
    pause
    exit /b 1
)

call gradle assembleDebug
echo.
echo Build complete!
echo APK location: app\build\outputs\apk\debug\app-debug.apk
pause
