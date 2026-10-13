@echo off
setlocal EnableExtensions DisableDelayedExpansion
pwsh -NoLogo -NoProfile -File "%~dp0Build.ps1" %*
exit /b %ERRORLEVEL%
