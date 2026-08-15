@echo off
setlocal EnableExtensions DisableDelayedExpansion

for %%I in ("%~dp0.") do set "R4OS_KERNEL_ROOT=%%~fI"
set "R4OS_SETTINGS=%R4OS_KERNEL_ROOT%\Settings.R4S"

if not exist "%R4OS_SETTINGS%" (
    echo ERROR: Settings file not found: "%R4OS_SETTINGS%"
    exit /b 1
)

set "R4OS_CONTRACT_SETTING="
set "R4OS_DEVKIT_SETTING="
set "R4OS_REPOSITORIES_SETTING="
set "R4OS_WORKSPACE_SETTING="
set "R4OS_ZIG_SETTING="

for /f "usebackq tokens=1,* delims==" %%A in ("%R4OS_SETTINGS%") do (
    if /i "%%A"=="CONTRACT_ROOT" set "R4OS_CONTRACT_SETTING=%%B"
    if /i "%%A"=="DEVKIT_ROOT" set "R4OS_DEVKIT_SETTING=%%B"
    if /i "%%A"=="REPOSITORIES_ROOT" set "R4OS_REPOSITORIES_SETTING=%%B"
    if /i "%%A"=="WORKSPACE_ROOT" set "R4OS_WORKSPACE_SETTING=%%B"
    if /i "%%A"=="ZIG_ROOT" set "R4OS_ZIG_SETTING=%%B"
)

for %%K in (WORKSPACE REPOSITORIES CONTRACT DEVKIT ZIG) do if not defined R4OS_%%K_SETTING (
    echo ERROR: %%K_ROOT is missing in "%R4OS_SETTINGS%".
    exit /b 1
)

pushd "%R4OS_KERNEL_ROOT%" >nul || exit /b 1
for %%I in ("%R4OS_WORKSPACE_SETTING%") do set "R4OS_WORKSPACE_ROOT=%%~fI"
for %%I in ("%R4OS_REPOSITORIES_SETTING%") do set "R4OS_REPOSITORIES_ROOT=%%~fI"
popd

pushd "%R4OS_REPOSITORIES_ROOT%" >nul || (
    echo ERROR: Repositories root not found: "%R4OS_REPOSITORIES_ROOT%"
    exit /b 1
)
for %%I in ("%R4OS_CONTRACT_SETTING%") do set "R4OS_CONTRACT_ROOT=%%~fI"
popd

pushd "%R4OS_WORKSPACE_ROOT%" >nul || (
    echo ERROR: Workspace root not found: "%R4OS_WORKSPACE_ROOT%"
    exit /b 1
)
for %%I in ("%R4OS_DEVKIT_SETTING%") do set "R4OS_DEVKIT_ROOT=%%~fI"
popd

pushd "%R4OS_DEVKIT_ROOT%" >nul || (
    echo ERROR: DevKit root not found: "%R4OS_DEVKIT_ROOT%"
    exit /b 1
)
for %%I in ("%R4OS_ZIG_SETTING%") do set "R4OS_ZIG_ROOT=%%~fI"
popd

if not exist "%R4OS_CONTRACT_ROOT%\build.zig.zon" (
    echo ERROR: Contract repository not found: "%R4OS_CONTRACT_ROOT%"
    exit /b 1
)

set "R4OS_ZIG_EXE=%R4OS_ZIG_ROOT%\zig.exe"
if not exist "%R4OS_ZIG_EXE%" (
    echo ERROR: Zig executable not found: "%R4OS_ZIG_EXE%"
    exit /b 1
)

pushd "%R4OS_KERNEL_ROOT%" >nul || exit /b 1
"%R4OS_ZIG_EXE%" build --fork="%R4OS_CONTRACT_ROOT%" %*
set "R4OS_EXIT_CODE=%ERRORLEVEL%"
popd

exit /b %R4OS_EXIT_CODE%
