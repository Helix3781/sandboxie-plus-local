@echo off
setlocal
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
if errorlevel 1 ( echo VCVARS_FAILED & exit /b 1 )
where cl
set "build_arch=x64"
set "PATH=D:\kaifa-soft\_build\Qt\6.8.4\msvc2022_64\bin;%PATH%"
cd /d D:\kaifa-soft\_build\Sandboxie\SandboxiePlus
echo === qmake ===
qmake SandboxiePlus.pro -r -spec win32-msvc "CONFIG+=release"
if errorlevel 1 ( echo QMAKE_FAILED & exit /b 1 )
echo === nmake ===
nmake > D:\kaifa-soft\_build\build_plus.log 2>&1
echo NMAKE_EXIT=%ERRORLEVEL%
