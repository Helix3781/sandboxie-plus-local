@echo off
setlocal enabledelayedexpansion
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" >nul

set WDK=D:\kaifa-soft\_build\wdk\pkg\c
set SDK=C:\Program Files (x86)\Windows Kits\10
set ROOT=D:\kaifa-soft\_build\Sandboxie\Sandboxie
set SHIM=D:\kaifa-soft\_build\drvshim
set OUT=%ROOT%\objdrv
set LOG=D:\kaifa-soft\_build\drv_build.log

set KMINC=%WDK%\include\10.0.26100.0\km
set WDKSHARED=%WDK%\include\10.0.26100.0\shared
set WDKUM=%WDK%\include\10.0.26100.0\um
set WDFINC=%WDK%\include\wdf\kmdf\1.35
set SDKSHARED=%SDK%\Include\10.0.26100.0\shared
set SDKUM=%SDK%\Include\10.0.26100.0\um
set SDKCRT=%SDK%\Include\10.0.26100.0\ucrt

set INCLUDE=%KMINC%;%KMINC%\crt;%WDKSHARED%;%WDKUM%;%WDFINC%;%SDKSHARED%;%SDKUM%;%SDKCRT%;%ROOT%;%ROOT%\core\drv;%ROOT%\msgs
set LIBPATH=%WDK%\lib\10.0.26100.0\km\x64

if not exist "%OUT%" mkdir "%OUT%"
del /q "%OUT%\*.obj" >nul 2>&1
del /q "%OUT%\*.res" >nul 2>&1
echo ===== DRIVER BUILD START =====> "%LOG%"

cd /d %ROOT%\core\drv

set CFLAGS=/c /nologo /GS- /GR- /EHs-c- /W3 /kernel /D_WIN64 /D_AMD64_ /DPOOL_NX_OPTIN=1 /DWIN9X_COMPAT_SPINLOCK /DUNICODE /D_UNICODE /DDEPRECATE_DDK_FUNCTIONS=1 /DMSC_NOOPT
set FORCEINC=/FI"%SDKSHARED%\warning.h"

set SRC=..\..\common\rbtree.c api.c box.c conf.c conf_expand.c conf_user.c dll.c driver.c dyn_data.c file.c file_xlat.c gui.c hook.c includes.c ipc.c ipc_lsa.c ipc_port.c ipc_sam.c ipc_spl.c key.c log.c mem.c obj.c process.c process_api.c process_force.c process_low.c process_util.c log_buff.c session.c syscall.c thread.c token.c util.c verify.c wfp.c

set FAILED=0
for %%f in (%SRC%) do (
  cl %CFLAGS% %FORCEINC% /Fo"%OUT%\\" %%f >> "%LOG%" 2>&1
  if errorlevel 1 (echo COMPILE_FAIL %%f>> "%LOG%" & set FAILED=1)
)
if "%FAILED%"=="1" (
  echo === COMPILE_ERRORS ===>> "%LOG%"
  findstr /C:"error C" "%LOG%"
  exit /b 1
)
echo === ALL C COMPILED OK ===>> "%LOG%"

rem --- ml64 for util_asm.asm (x64 build) ---
ml64 -c -Cx -nologo -D_WIN64 -Zi -Fo"%OUT%\util_asm.obj" util_asm.asm >> "%LOG%" 2>&1
if errorlevel 1 (echo ML64_FAIL >> "%LOG%" & exit /b 1)

rem --- resource ---
rc /nologo /D_WIN32 /I "%SHIM%" /I "%SDKUM%" /I "%ROOT%" /fo "%OUT%\driver.res" resource.rc >> "%LOG%" 2>&1
if errorlevel 1 (echo RC_FAILED >> "%LOG%" & exit /b 1)
echo === RC OK ===>> "%LOG%"

rem --- link ---
link /nologo /DRIVER /SUBSYSTEM:NATIVE /ENTRY:DriverEntry /INTEGRITYCHECK /OPT:REF /OPT:ICF /DEBUG /OUT:"%OUT%\SbieDrv.sys" /LIBPATH:"%LIBPATH%" ntoskrnl.lib hal.lib wmilib.lib fltmgr.lib Ksecdd.lib Fwpkclnt.lib ntstrsafe.lib BufferOverflowFastFailK.lib "%OUT%\*.obj" "%OUT%\driver.res" >> "%LOG%" 2>&1
echo LINK_EXIT=%errorlevel%>> "%LOG%"
if errorlevel 1 (
  echo === LINK_ERRORS ===
  findstr /C:"error" "%LOG%"
  exit /b 1
)
echo === LINK OK ===> "%LOG%"
dir "%OUT%\SbieDrv.sys" >> "%LOG%" 2>&1
echo BUILD_SUCCESS
