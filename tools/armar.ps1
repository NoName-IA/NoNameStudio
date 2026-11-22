# Arma NoName Studio: .exe de Windows y .apk de Android, y los copia a dist\ y al escritorio.
$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
Set-Location $root
$env:PATH = "$env:LOCALAPPDATA\Programs\winlibs\mingw64\bin;C:\Program Files\Java\jdk-24\bin;$env:PATH"
$env:ANDROID_HOME = "$env:LOCALAPPDATA\Android\Sdk"
$env:ANDROID_SDK_ROOT = $env:ANDROID_HOME
$env:ANDROID_NDK_ROOT = "$env:ANDROID_HOME\ndk\27.2.12479018"
$env:JAVA_HOME = "C:\Program Files\Java\jdk-24"

cargo build --release
cargo apk build --release --lib

New-Item -ItemType Directory -Force dist, "$env:USERPROFILE\Desktop\NoName Studio" | Out-Null
Copy-Item target\release\NoNameStudio.exe, target\release\apk\NoNameStudio.apk dist\ -Force
Copy-Item dist\* "$env:USERPROFILE\Desktop\NoName Studio\" -Force
Get-ChildItem dist | Format-Table Name, Length
