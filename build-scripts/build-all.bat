@echo off
REM Refresh - build everything (Windows host .exe + Android .apk)
REM Double-click this, or run it from a terminal, from anywhere -
REM it locates the project root itself.

setlocal
set SCRIPT_DIR=%~dp0

echo === Refresh: build-all ===
echo.

powershell -NoProfile -ExecutionPolicy Bypass -File "%SCRIPT_DIR%build-windows-host.ps1"
if errorlevel 1 (
    echo.
    echo Windows host build failed - see the errors printed above this line.
    echo ^(This window stays open - press any key to close it.^)
    pause
    exit /b 1
)

echo.
powershell -NoProfile -ExecutionPolicy Bypass -File "%SCRIPT_DIR%build-android.ps1"
if errorlevel 1 (
    echo.
    echo Android build failed - see the errors printed above this line.
    echo The Windows host build above still succeeded; dist\refresh-windows-host.exe is usable on its own.
    echo ^(This window stays open - press any key to close it.^)
    pause
    exit /b 1
)

echo.
echo === All builds finished. See the dist\ folder. ===
pause
