@echo off
setlocal enabledelayedexpansion
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" >nul
set WDK=D:\kaifa-soft\_build\wdk\pkg\c
set SDK=C:\Program Files (x86)\Windows Kits\10\Include\10.0.26100.0
set ROOT=D:\kaifa-soft\_build\Sandboxie\Sandboxie
set OUT=%ROOT%\objdrv
set INCLUDE=%WDK%\include\10.0.26100.0\km;%WDK%\include\10.0.26100.0\km\crt;%WDK%\include\10.0.26100.0\shared;%WDK%\include\10.0.26100.0\um;%WDK%\include\wdf\kmdf\1.35;%SDK%\shared;%SDK%\ucrt;%ROOT%;%ROOT%\core\drv;%INCLUDE%
cd /d %ROOT%\core\drv
cl /c /nologo /GS- /GR- /EHs-c- /W3 /D_WIN64 /D_AMD64_ /DPOOL_NX_OPTIN=1 /DWIN9X_COMPAT_SPINLOCK /DUNICODE /D_UNICODE /FI"%SDK%\shared\warning.h" /Fo"%OUT%\\" ipc_port.c > "%OUT%\ipc_port.log" 2>&1
echo EXIT=%errorlevel%
findstr /C:"error" "%OUT%\ipc_port.log"
