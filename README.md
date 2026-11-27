# Sandboxie-Plus 本地自用改造集（1.19.1）

基于 [Sandboxie-Plus](https://github.com/sandboxie-plus/Sandboxie) **v1.19.1** 的个人自用补丁集。
**非官方项目**，与 Sandboxie 原作者无关，仅供本人本机非商业使用。

本仓库只发布**源码**，不发布任何编译产物或二进制。

## 为什么改

原版在「沙箱内程序被自动清除/系统更新」之后，用 SandMan 恢复快照会报
`zwork.kernel.boot_failed: runtime_boot_failed`（底层是 `ENOENT: chmod ...\runtime\artifacts`）。
根因在 `SandMan` 的快照机制：`TakeSnapshot` 把 `drive/user/share` 整体 move 进 `snapshot-N`（变成只读下层），
而 `SelectSnapshot` 只恢复注册表并把三个目录清空，**一个文件都不铺回**，指望驱动把 `snapshot-N` 当只读下层合并视图。
一旦父层放程序、子层放数据，合并视图能看到文件但 `chmod` 就 `ENOENT`。

顺带按自用需求做了几处收敛（详见下表）。

## 目录结构

```
upstream/          v1.19.1 官方原始源码，未作任何改动，仅作对照与 diff 基线
local/             我们改造后的源码（= upstream + 下表 15 个文件的改动）
sbie-box/          Rust 编写的本地 CLI + MCP 服务（新增，与 C++ 部分独立）
docs/              中文使用说明（绿色程序导入箱内、开机自启等）
build-scripts/     本次实际使用的构建/签名/打补丁脚本（不在官方构建体系内）
```

`upstream/` 与 `local/` **都是纯源码**，不含 `Bin/`、`obj*/`、`Makefile`、`*.obj/*.lib/*.pdb` 等任何构建产物。
比对两棵树即可得到全部改动：

```bat
git diff --no-index --stat upstream local
```

## 改动清单

| # | 目的 | 文件 | 做法 |
|---|------|------|------|
| 1 | 恢复快照时**全量物化**文件 | `SandboxiePlus/QSbieAPI/Sandboxie/SandBox.cpp`<br>`SandboxiePlus/QSbieAPI/Sandboxie/SandBox.h` | 重写 `CSandBox::SelectSnapshot`：沿 `Snapshot_N/Parent` 回溯祖先链，按层先应用该层 `FilePaths.dat` 里的删除记录，再逐层递归拷贝 `drive/user/share`，最后写 `Current/Snapshot`。因为 `CleanBoxFolders()` 是异步的，改为 `SelectSnapshotAsync` + `QtConcurrent::run`，返回真正的 `OP_ASYNC` 进度对象 |
| 2 | 修配套崩溃 | `SandboxiePlus/SandMan/SandMan.cpp` | `CSandMan::AddAsyncOp` 增加 `pProgress.isNull()` 防御，避免空进度对象解引用 |
| 3 | 去掉付费授权位（驱动侧） | `Sandboxie/core/drv/verify.c` | `KphValidateCertificate()` 的 `CleanupExit` 路径无条件置 `active=1`、`opt_desk/opt_net/opt_enc/opt_sec=1`、`type=eCertEternal` |
| 4 | 去掉对厂商签名的强依赖 | `Sandboxie/core/drv/util.c` | `MyIsCallerSigned()` 直接 `return TRUE`（等价于永远处于 test signing 放行状态） |
| 5 | 箱类型收敛为两种 | `SandboxiePlus/SandMan/Wizards/NewBoxWizard.cpp` | 建箱向导只保留「标准沙盒」与「应用程序隔离箱」（不带数据保护） |
| 6 | 移除证书/评估证书引导 | `SandboxiePlus/SandMan/Wizards/SetupWizard.cpp` | 注释掉 `Page_Certificate` 注册，向导不再进入证书页 |
| 7 | MCP / AI 页面本地化 | `SandboxiePlus/SandMan/Mcp/McpClientSetup.cpp`<br>`SandboxiePlus/SandMan/Mcp/McpConnectDialog.cpp`<br>`SandboxiePlus/SandMan/Windows/SettingsWindow.cpp`<br>`SandboxiePlus/SandMan/Forms/SettingsWindow.ui` | 服务器名改 `sandboxie-local`，优先指向同目录 `sbie-mcp.exe`；第三方客户端条目换成「本地 JSON 配置」与「sbie-box 命令行」两条；AI 页小标题修正并追加 CLI 说明 |
| 8 | 内置「导入程序」 | `SandboxiePlus/SandMan/Views/SbieView.cpp`<br>`SandboxiePlus/SandMan/Views/SbieView.h` | 箱右键 → **Box Content → Import Program...**，主菜单 **Sandbox → Import Program**：把绿色程序整个文件夹拷进箱内 `drive\<盘>\<原路径>` 并在运行菜单里加一条入口（路径含空格时用引号写入，不需要 8.3 短名）。 |
| 8 | 适配无 ATL / Qt6 的构建环境 | `SandboxiePlus/SandMan/Helpers/WinHelper.cpp`<br>`SandboxiePlus/SandMan/Helpers/AtlShim.h`（新增）<br>`SandboxiePlus/SandMan/Views/FileView.cpp`<br>`SandboxiePlus/SandMan/SandMan.qc.pro` | 自带最小 `CComPtr`/`CComHeapPtr` 垫片替代 `atlbase.h`；`QImage::fromHICON` 自实现；去掉 `QT += winextras` |

### 列级说明（为什么要动驱动）

`Sandboxie/core/drv/process.c:839` 处，只要 `!(Verify_CertInfo.active && Verify_CertInfo.opt_sec)`，
且箱开了 `UseSecurityMode / SysCallLockDown / RestrictDevices / UseRuleSpecificity / UsePrivacyMode / NoSecurityIsolation / ProtectHostImages`
任意一项，驱动就会 `Log_Msg_Process(MSG_6004)` 并 `Process_ScheduleKill(proc, 5*60*1000)` ——
**5 分钟后由内核杀进程**，改 UI 绕不过去。所以「应用程序隔离箱能正常跑」必须在驱动层解锁。

## Rust 工具（`sbie-box/`）

纯本地，零网络依赖，只依赖 `serde`/`serde_json`。

- `sbie-box.exe` —— 命令行：`list / status / snapshots / restore / take / snap-delete / boot / run / stop / create / delete / import / autorun / autostart`，所有命令输出 JSON。`autorun <box>` 跑一次箱内 Windows 启动项（没有条目时返回 `nothingToRun: true`）；`autostart <box> [on|off]` 开关 `[GlobalSettings] StartSystemBox`（整箱随开机自启）。`import <box> <folder>` 把绿色程序整体搬进箱内并在运行菜单里加一条入口（路径含空格时自动改用 8.3 短名，因为 Sandboxie 的 `RunCommand` 不支持带引号的命令）。
- `sbie-mcp.exe` —— MCP 服务端，stdio + 换行分隔 JSON-RPC 2.0（protocolVersion `2024-11-05`），14 个工具：`sbie_list_boxes`、`sbie_box_status`、`sbie_list_snapshots`、`sbie_restore_snapshot`、`sbie_take_snapshot`、`sbie_delete_snapshot`、`sbie_boot`、`sbie_run`、`sbie_terminate`、`sbie_create_box`、`sbie_delete_box`、`sbie_import_program`、`sbie_autorun`、`sbie_autostart`。

要点：
- 写 `C:\Windows\Sandboxie.ini` 一律**经 `SbieIni.exe`**（走 `SbieIniServer` 以 SYSTEM 身份落盘并自动 reload 驱动配置），普通权限直接写会被拒。
- 驱动读的是**箱级** `FileRootPath`/`BoxRootFolder`，所以建箱时会显式写 `FileRootPath=<全局 BoxRootFolder>\%SANDBOX%`。
- 快照物化逻辑与 C++ 侧一致，可作为命令行替代路径（本次就是用它把被清空的箱子救回来的）。

构建：

```bat
cd sbie-box
cargo build --release
```

## 界面里的三项本地增强

| 位置 | 名称 | 行为 |
|---|---|---|
| 箱右键 → Box Content / 主菜单 Sandbox | **导入文件夹…** | 整个目录拷进箱内 `drive\<盘>\<原路径>`，顶层唯一 exe 自动挂进运行菜单，多个则让你选 |
| 同上 | **导入程序…** | 单个 exe 拷进箱内并挂进运行菜单（绿色程序只有一个文件时用这个） |
| 箱右键 → 运行 | **运行指定程序…** | 弹文件框直接挑 exe，默认从该箱 `drive` 开始并记住每箱上次目录 |
| 箱右键 → 运行 | **执行自动运行项目…** | 跑箱内 Windows 启动项（注册表 Run/RunOnce + 启动文件夹）；执行前弹出说明框并列出条目，空箱会明示「（空）」 |

前两项与「运行指定程序…」共用一条规则：**程序已在箱内 → 运行命令指向箱内副本；未导入 → 仍指向宿主路径**（`CSandBoxPlus::MakeBoxAwareCommand` / `GetBoxAwareFullCommand`）。

## 构建（Windows / MSVC）

官方构建体系需要 Qt6 + WDK，本机环境没有 WDK 的 MSBuild toolset，因此改用脚本直接调 `cl/ml64/rc/link`。
`build-scripts/` 里是本次实际用过的脚本，可作参考：

| 脚本 | 作用 |
|------|------|
| `build_plus.cmd` | 用户态：`qmake -r` + `nmake` 编 `SandboxiePlus.pro`（SandMan / QSbieAPI / MiscHelpers / SbieMcp） |
| `build_ugh.cmd` | 单独编 `UGlobalHotkey` 子工程（未纳入 SUBDIRS） |
| `gen_msgs.cmd` | 用 `msgs/parse.c` 生成 `msgs.mc` 再 `mc` 出 `msgs.h`（驱动依赖） |
| `build_drv.cmd` | 驱动：按 `SbxDrv.vcxproj` 的 `ExcludedFromBuild` 过滤出 36 个 `.c`，手工 `cl /kernel` → `ml64` → `rc` → `link /DRIVER` |
| `drv_list.py` | 解析 vcxproj，输出该配置下真正要编的源文件清单 |
| `make_cert3.ps1` | 生成两级自签 PKI（根 CA + 代码签名叶证书，EKU 为内核代码签名）并签名全部产物 |
| `mk_testenv.ps1` | 合成一个假箱子（两层快照 + `FilePaths.dat` 删除记录）用于离线验证物化逻辑 |
| `patch_newbox.ps1`<br>`patch_rust_ini.py`<br>`patch_snapshot_async.py` | 本次改动用到的定位替换脚本，留作改动溯源 |

编译顺序（用户态）：`MiscHelpers` → `QSbieAPI` → `qtsingleapp` → `SandMan` → `SbieMcp`。
直接 `nmake` 顶层 Makefile 会因为把 `SandMan` 排在依赖前面而报 `LNK1181: 无法打开输入文件 QSbieAPI.lib`。

## 安装与验证（本机）

需要管理员权限（装驱动 / `bcdedit` 改 testsigning 都必须提权）：

1. 开 testsigning：`bcdedit /set {current} testsigning on`
2. 把自签根 CA 导入 `LocalMachine\TrustedRoot`，叶证书导入 `LocalMachine\My`
3. 停 `SandMan` / `SbieSvc` / `SbieDrv`，替换 `C:\Program Files\Sandboxie-Plus\` 下的
   `SandMan.exe`、`SbieMcp.exe`、`QSbieAPI.dll`、`MiscHelpers.dll`、`UGlobalHotkey.dll`、`SbieDrv.sys`，
   以及 `Tools\` 下的 `sbie-box.exe` / `sbie-mcp.exe`
4. 驱动被内核占用时，用 `PendingFileRenameOperations` 登记「下次启动替换」
5. **重启电脑**（不是重启程序）

验证点：
- `bcdedit /enum {current}` 显示 `testsigning Yes`
- `sbie-box.exe list` 能列出箱
- SandMan 不再弹「does not have a valid signature」
- 切快照弹进度框且不崩，恢复后 `drive/user` 里有真实文件
- 应用程序隔离箱里跑程序**超过 5 分钟不被杀**

## 已知限制

- 必须关闭 Secure Boot 并使用 testsigning；桌面右下角会有「测试模式」水印。
- 自签证书 `signtool verify /kp` 必然报「不链到 Microsoft Root」——这是自签的固有结果，testsigning 下不影响加载。
- 不要用 `SandMan` 自带的「检查更新」/在线证书入口（本改造已移除大部分入口，但请勿联网使用）。
- 不要在 SandMan 运行时用 CLI 建箱/删箱：SandMan 持有自己的箱模型，会把外部新增的箱挤掉。
- 驱动为本地手工编译，未做优化配置，性能不代表官方。

## 许可与合规

- `upstream/` 与 `local/` 保留全部原始版权声明。Sandboxie 内核与经典组件按 **GPL-3.0**（`LICENSE.Classic`）；
  SandMan / 沙箱 UI 部分适用 `LICENSE.Plus` 的自定义条款 —— 其中明确允许「修改、编译并以源码形式再分发」，
  公开再分发自编译二进制时则要求「全部修改后的源码公开、软件名称不得更改、不得移除作者与版权信息」。
- 本仓库因此**以源码为主体**、不改软件名称（二进制内仍为 `SandMan` / `SbieDrv`，窗口标题仍为 Sandboxie-Plus）、
  不移除任何版权信息；Release 里挂的自编译二进制仅作本机验证与合规存档，与源码同一许可。
- 私钥（`.pfx`）与签名材料**不入库**，由 `build-scripts/make_cert3.ps1` 在本机生成。
- 仅个人非商业使用，无任何担保。

## 文档

- [开发与构建经验总结](docs/开发与构建经验总结.md) —— 从零搭构建环境开始，一路到驱动/Qt 编译坑、授权门禁机理、两级自签 PKI 与安装、快照物化与 UI 崩溃修复、Rust CLI 的 ini 提权与 8.3 短名、本机 shell 的 8 个坑。**想自己复现编译的人先看这篇。**
- [使用说明：绿色程序常驻与开机自启](docs/Sandboxie使用说明-绿色程序常驻与开机自启.md) —— 界面操作级说明：导入文件夹/导入程序、运行指定程序、强制目录、开机自启。

## 编译产物

本仓库不放二进制（只放源码）。可直接下载已编译好的整包：

- [Release v1.19.1-local.2](https://github.com/Helix3781/sandboxie-plus-local/releases/tag/v1.19.1-local.2) —— `SandboxieLocal-1.19.1-win64.zip`（含 SandMan/驱动/证书/安装脚本/Rust CLI/文档），安装需管理员并重启电脑。
- 更早版本：[v1.19.1-local.1](https://github.com/Helix3781/sandboxie-plus-local/releases/tag/v1.19.1-local.1)。

> 该二进制是**个人自用的本地构建**，非官方发布；仅供本机验证，请勿分发。
