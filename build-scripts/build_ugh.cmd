@echo off
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" >nul
set PATH=D:\kaifa-soft\_build\Qt\6.8.4\msvc2022_64\bin;%PATH%
set build_arch=x64
cd /d D:\kaifa-soft\_build\Sandboxie\SandboxiePlus\UGlobalHotkey
D:\kaifa-soft\_build\Qt\6.8.4\msvc2022_64\bin\qmake.exe uglobalhotkey.qc.pro -spec win32-msvc "CONFIG+=release" -o Makefile.ugh
if errorlevel 1 (echo QMAKE_FAILED & exit /b 1)
nmake -f Makefile.ugh > D:\kaifa-soft\_build\ugh.log 2>&1
if errorlevel 1 (echo BUILD_FAILED & exit /b 1)
echo UGH_OK
dir /b ..\Bin\x64\Release\UGlobalHotkey.lib
