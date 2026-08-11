@echo off
setlocal
cd /d "%~dp0"

echo == Aadt.Bridge (managed) ==
dotnet build Aadt.Bridge\Aadt.Bridge.csproj -c Release
if errorlevel 1 exit /b 1

echo == Aadt.Bridge.Smoke ==
dotnet build Aadt.Bridge.Smoke\Aadt.Bridge.Smoke.csproj -c Release
if errorlevel 1 exit /b 1

echo == Aadt.Bridge.Native (AOT shared lib aadt_bridge.dll) ==
dotnet publish Aadt.Bridge.Native\Aadt.Bridge.Native.csproj -c Release -r win-x64
if errorlevel 1 exit /b 1

set OUT=Aadt.Bridge.Native\bin\Release\net8.0\win-x64\publish\aadt_bridge.dll
if not exist "%OUT%" (
  echo ERROR: missing %OUT%
  exit /b 1
)

set WINUI_DBG=..\native\aad-winui\winui\AadWinui\x64\Debug\AadWinui
set WINUI_REL=..\native\aad-winui\winui\AadWinui\x64\Release\AadWinui
if exist "%WINUI_DBG%" copy /Y "%OUT%" "%WINUI_DBG%\" >nul
if exist "%WINUI_REL%" copy /Y "%OUT%" "%WINUI_REL%\" >nul

echo OK %OUT%
endlocal
