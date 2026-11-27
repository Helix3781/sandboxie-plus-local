@echo off
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" >nul
cd /d D:\kaifa-soft\_build\Sandboxie\Sandboxie\msgs
if not exist SbieRelease mkdir SbieRelease
del /q SbieRelease\parse.exe 2>nul
cl /nologo /O2 /D_CRT_NON_CONFORMING_SWPRINTFS /DUNICODE /D_UNICODE /I .. parse.c /Fe:SbieRelease\parse.exe /Fo:SbieRelease\
if errorlevel 1 (echo PARSE_BUILD_FAILED & exit /b 1)
copy /y Sbie-English-1033.txt Text-English-1033.txt >nul
setlocal enabledelayedexpansion enableextensions
set PARSE_ARGS=
for %%f in (Text-*.txt) do (set PARSE_ARGS=%%f !PARSE_ARGS!)
SbieRelease\parse !PARSE_ARGS! 2> ParseOutput.log
if errorlevel 1 (echo PARSE_RUN_FAILED & type ParseOutput.log & exit /b 1)
mc /h SbieRelease /r SbieRelease -u msgs.mc
echo MC_EXIT=%errorlevel%
dir SbieRelease\msgs.h
