# build-scripts

本次改造实际用到的构建/签名/打补丁脚本，**不在官方构建体系内**，仅作参考与改动溯源。

请把它们复制到 `D:\kaifa-soft\_build\`（或任何工作目录）后按需修改其中的绝对路径再运行；
脚本里的 `D:\kaifa-soft\_build\...` 都是本机路径，直接换机运行前必须先改。

## 环境前提

- Visual Studio 2022 Build Tools（含 MSVC v143），`vcvars64.bat` 可用
- Qt 6.8.4 msvc2022_64（自定义构建，`qmake` 在 `bin\` 下）
- WDK 内核头/库（本机用 nuget 包 `microsoft.windows.wdk.x64` 解压，未装 MSI）
- Windows SDK（提供 `shared\warning.h`、`um`、`ucrt`）
- Rust（`cargo`）用于 `sbie-box/`

## 脚本

| 脚本 | 说明 |
|------|------|
| `build_plus.cmd` | `call vcvars64.bat` → PATH 前置 Qt6 `bin` → `cd SandboxiePlus` → `qmake SandboxiePlus.pro -r -spec win32-msvc "CONFIG+=release"` → `nmake`。**必须整份 .bat 执行**，把多行压成一行 `cmd /c` 会让 `%PATH%` 提前展开，导致 `Project ERROR: Cannot run compiler 'cl'` |
| `build_ugh.cmd` | 单独编 `UGlobalHotkey`（该目录不在 SUBDIRS 里）：`set build_arch=x64` + `qmake uglobalhotkey.qc.pro ...` + `nmake -f Makefile.ugh` |
| `gen_msgs.cmd` | 生成驱动依赖的消息头：编 `msgs/parse.c`（需 `/I ..`、`/D_CRT_NON_CONFORMING_SWPRINTFS`、`/DUNICODE /D_UNICODE`）跑出 `msgs.mc`，再 `mc /h SbieRelease /r SbieRelease -u msgs.mc` |
| `build_drv.cmd` | 驱动手工构建：按 `ExcludedFromBuild` 过滤出 36 个 `.c` → `cl /kernel ...` → `ml64 util_asm.asm` → `rc resource.rc` → `link /DRIVER /SUBSYSTEM:NATIVE /ENTRY:DriverEntry /INTEGRITYCHECK` |
| `try_ipc.cmd` | 单独复现 `ipc_port.c` 的编译（验证 `/FI ...\shared\warning.h` 这一关键修正） |
| `drv_list.py` | 用 `xml.etree` 解析 `SboxDrv.vcxproj`，输出指定配置下真正参与编译的源文件清单 |
| `make_cert3.ps1` | 生成两级自签 PKI 并签名：根 CA（`CA=true` critical，EKU 内核代码签名）→ 叶证书；`signtool sign /sha1 <leaf> /ac <root.cer>`。必须在 **PowerShell 7** 下跑（5.1 缺 `RSACng`） |
| `mk_testenv.ps1` | 造一个合成测试箱（两层快照 + `FilePaths.dat` 删除记录），用来离线验证快照物化逻辑，不碰真实箱子 |
| `patch_newbox.ps1` | 按内容锚点改 `NewBoxWizard.cpp` 的箱类型列表 |
| `patch_rust_ini.py` | 给 `sbie-box` 加 `SbieIni.exe` 写入层与建箱默认键 |
| `patch_snapshot_async.py` | 把 `SelectSnapshot` 改成异步物化 + 抽 `SelectSnapshotAsync` |

## 关键坑（省得再踩）

1. **多行命令不要压成一行 `cmd /c`**：`%PATH%` 会在整行解析时展开，vcvars 的结果被覆盖。
2. **`cmd //c "...bat"` 在本环境无效**（只打印横幅就退出）。要用
   `powershell -NoProfile -ExecutionPolicy Bypass -Command "cmd /c 'D:\...\x.cmd'"`。
3. **驱动不能按 ClCompile 全量编**，必须按配置过滤（x64 只有 36 个）。
4. **`try/__except` 宏**来自 SDK 的 `shared\warning.h`，WDK 包里没有 → `cl` 必须 `/FI` 强制包含它。
5. **`resource.rc` 依赖 `afxres.h`**，无 ATL 环境需要一个只 include `winres.h`/`winver.h` 的垫片。
6. **链接器要加 `BufferOverflowFastFailK.lib`**（在 WDK `lib\10.0.26100.0\km\x64\`），因为驱动开了 `/GS-` 但上游引用了 fastfail。
7. **签名要指定叶证书**：PFX 里有多张证书，`signtool sign` 必须 `/sha1 <leaf-thumbprint>`，并 `/ac <root.cer>` 把根挂进签名块。
