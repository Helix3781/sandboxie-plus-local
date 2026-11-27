# Sandboxie Local Build 1.19.1

本机自用的 Sandboxie-Plus 改造版：**内核与 UI 沿用官方 1.19.1 源码，CLI 与 MCP 用 Rust 重写**。
无任何联网、无证书授权、无付费箱类型。

---

## 一、改了什么（对照需求）

| # | 需求 | 实现 |
|---|------|------|
| 1 | MCP 及相关 AI 功能本地化、不联网 | MCP 走**命名管道**（`\Device\NamedPipe\SbieMcp*`），不含网络 socket；新增 Rust `sbie-mcp.exe`（纯 stdio，零网络依赖）。唯一的两个外链只是「手动打开浏览器提 issue」的文案，不影响运行。 |
| 2 | 没有 CLI 就补 CLI | 新增 Rust `sbie-box.exe`：列箱/状态/快照增删与还原/boot/run/stop/建箱/删箱/导入绿色程序，输出 JSON。设置页的 AI 一栏会直接显示它的路径。 |
| 8 | 内置「导入文件夹 / 导入程序」菜单 | 箱右键 → **Box Content**、或主菜单 **Sandbox**，两项：**导入文件夹…**（整个目录拷进箱内 `drive\<盘>\<原路径>`，自动挑顶层唯一 exe 或让你选一个挂进运行菜单）与 **导入程序…**（单个 exe 拷进箱内并挂进运行菜单）。路径含空格时用引号写入，不需要 8.3 短名。  启动时自动判断：若该程序已在箱内（`<箱根>\drive\<盘>\…` 存在），运行菜单指向**箱内副本**；否则仍指向宿主路径。 |
| 3 | 移除需要授权的付费功能 | 驱动 `verify.c` 在证书校验的收口处无条件置 `active=1, opt_sec/opt_enc/opt_net/opt_desk=1, type=eCertEternal`（所有功能位永久解锁）；`util.c` 的 `MyIsCallerSigned()` 直接放行（连 testsigning 都不再是前提）；删除首次启动向导里的「支持证书 / 免费评估证书」页。 |
| 4 | 只保留「标准沙盒」+「应用程序沙盒（应用程序隔离）」 | `NewBoxWizard` 箱类型列表只剩两项：`Standard Sandbox`、`Application Compartment Box`（**不带数据保护的版本**，`eAppBox`）。 |
| 5 | 这两种类型各建一个 | `StandardBox`（纯默认键）与 `AppCompartmentBox`（`NoSecurityIsolation=y` + `Template=RpcPortBindingsExt`，**不写** `UsePrivacyMode`）。 |
| 6 | 修快照恢复后起不来的问题 | 重写 `CSandBox::SelectSnapshot`：不再依赖驱动把快照当只读下层，而是沿父链**全量物化** drive/user/share（含 `FilePaths.dat` 删除记录的应用），恢复后目录真实存在。 |
| 7 | MCP 客户端配置与本地实际匹配 | 服务器名改为 `sandboxie-local`；命令优先指向本安装的 `sbie-mcp.exe`（缺失时才回落 `SbieMcp.exe`）；「Connect an AI Agent」里的第三方客户端列表（Claude Desktop / Claude Code / VS Code）换成两项本地条目：`MCP client (JSON configuration)` 与 `Command line (sbie-box)`。 |

---

## 二、目录结构

```
SandboxieLocal-1.19.1/
├─ bin/          SandMan.exe / SbieMcp.exe / QSbieAPI.dll / MiscHelpers.dll / UGlobalHotkey.dll
│                 + sbie-box.exe（CLI） / sbie-mcp.exe（MCP 服务）
├─ driver/       SbieDrv.sys（已自签）
├─ cert/         SandboxieLocalRootCA.cer（根 CA） / SandboxieLocalCodeSigning.cer（叶证书）
└─ scripts/      install-local.ps1（管理员安装/回滚）
```

`bin/` 下 8 个文件都已用「根 CA → 叶证书」两级链签名，`Get-AuthenticodeSignature` 全部为 `Valid`。

---

## 三、安装（需要管理员）

前提：**Secure Boot 已关闭**（本机已确认关闭）；安装后**必须重启**。

```powershell
# 以管理员身份
pwsh -File .\scripts\install-local.ps1
```

脚本会：
1. `bcdedit /set {current} testsigning on`，并回读 BCD 校验
2. 清掉旧的同名证书，把根 CA 装入 `LocalMachine\TrustedRoot`、叶证书装入 `LocalMachine\My`
3. 备份将被替换的文件到 `C:\Program Files\Sandboxie-Plus\LocalBuildBackup`（已有备份不覆盖）
4. 停 SandMan（此时还留着 SbieSvc），**用 `sbie-box.exe` 建好两个箱**（已存在则跳过）
5. 停 SbieSvc / SbieDrv，替换 `bin/`、`driver/` 的文件，逐个复验签名
6. 把 `sbie-box.exe`、`sbie-mcp.exe` 装到 `C:\Program Files\Sandboxie-Plus\Tools\`
7. 打印 `REBOOT IS REQUIRED`

> 驱动文件若仍被内核占用，脚本会自动登记 `PendingFileRenameOperations`，**重启时**完成替换。
> 重启前 SandMan 报「does not have a valid signature」是预期现象——旧驱动还在内存里。

回滚：

```powershell
pwsh -File .\scripts\install-local.ps1 -Rollback
bcdedit /set {current} testsigning off
```

---

## 四、CLI 用法

```powershell
$SB = "C:\Program Files\Sandboxie-Plus\Tools\sbie-box.exe"

& $SB list                              # 列出所有箱
& $SB status AppCompartmentBox          # 箱状态 + 运行中的 PID
& $SB snapshots Autoclaw                # 快照列表（含父链与时间）
& $SB restore Autoclaw 2 --wait-ms 60000
& $SB take Autoclaw "装完补丁"
& $SB boot Autoclaw --snapshot 2 --exe "C:\Program Files\X\X.exe" --wait-window X
& $SB run StandardBox --exe "C:\Windows\System32\notepad.exe"
& $SB stop StandardBox
& $SB create NewBox --type standard     # 或 --type app
& $SB delete NewBox --purge
& $SB import AppCompartmentBox "D:\Green\Foo"   # 把绿色程序整体搬进箱内 + 加运行菜单
& $SB import AppCompartmentBox "D:\Green\Foo" --exe Foo.exe --name "我的工具"
```

全部输出 JSON。`create` 一次写入与 SandMan 一致的完整默认键（`ConfigLevel=10`、7 个默认模板、`FileRootPath` 等）；`delete` 会同时把它从 `BoxGrouping` 里摘掉。
箱名只允许 `0-9 A-Z a-z _`，最长 37 字符（驱动 `Box_IsValidName` 的限制）。

`import` 把绿色程序**实体**拷进箱内 `<箱根>\drive\<盘>\<原路径>`（箱内进程看到的路径不变），并在运行菜单里加一条入口；路径含空格时自动改用 8.3 短名，因为 Sandboxie 的 `RunCommand` 不支持带引号的命令。`--no-menu` 只拷贝不加菜单。

---

## 五、MCP 用法

两种服务端任选：
- `SbieMcp.exe`（官方 Qt shim，把 stdio 桥到 SandMan 的命名管道；需要 SandMan 在跑）
- `sbie-mcp.exe`（Rust，自带全部实现，不依赖 SandMan）

SandMan 的「Connect an AI Agent」对话框已改为本地化版本：服务器名 `sandboxie-local`，命令优先指向 `sbie-mcp.exe`，并给出可直接粘贴的 JSON 片段与 `sbie-box` 命令行说明。

Rust 版注册示例（任何 MCP 客户端）：

```json
{
  "mcpServers": {
    "sandboxie-local": {
      "command": "C:\\Program Files\\Sandboxie-Plus\\Tools\\sbie-mcp.exe",
      "args": []
    }
  }
}
```

对外暴露 12 个工具：`sbie_list_boxes`、`sbie_box_status`、`sbie_list_snapshots`、`sbie_restore_snapshot`、`sbie_take_snapshot`、`sbie_delete_snapshot`、`sbie_boot`、`sbie_run`、`sbie_terminate`、`sbie_create_box`、`sbie_delete_box`、`sbie_import_program`。
**全部本地执行，不发任何网络请求。**

---

## 六、重启后验证清单

1. `sbie-box.exe list` 能看到 `StandardBox` / `AppCompartmentBox`。
2. SandMan 不再提示「没有有效的签名」，两个箱都正常显示（应用隔离箱是绿色的 Compartment 类型）。
3. 在 **AppCompartmentBox** 里跑一个程序，**超过 5 分钟不被杀**（关键：验证驱动解锁生效）。
4. 在一个有快照的箱子上 `sbie-box restore <box> <snap>`，箱根 `drive/`、`user/` 有真实文件（不再是空目录）。
5. 设置 → AI Integration：小标题为 `AI Agent (MCP)`，底部能看到 `sbie-box.exe` 的路径；「连接 AI 代理」里只有两个本地条目。

---

## 七、限制与注意

- 启用 testsigning 后桌面右下角会有「测试模式」水印，属正常现象。
- 本机必须保持 Secure Boot 关闭，否则自签驱动无法加载。
- 自签证书不会链到微软根，`signtool verify /kp` 必定报错，这不影响 testsigning 加载。
- 驱动为无优化编译（`/Od`），功能完整、体积略大，性能非关键路径。
- 私钥 `SandboxieLocalCodeSigning.pfx`（密码 `sandboxie-local`）保存在构建目录 `D:\kaifa-soft\_build\cert\`，**不要外传**。
- 不要在 SandMan 运行时用 CLI 建箱/删箱：SandMan 持有自己的箱模型并会回写 ini，可能把外部新增的箱丢掉。安装脚本已把建箱放在 SandMan 停止之后。

## 常见问题：启动时提示「由于找不到 qtsingleapp.dll，无法继续执行代码」

`SandMan.exe` 是硬依赖 `qtsingleapp.dll` 的。出现这个系统错误对话框，说明 SandMan.exe 被放进了一个
**缺少 qtsingleapp.dll（以及 Qt6*.dll、msvcp140 等运行库）的目录**：

* 正式使用不要直接双击 `bin\SandMan.exe`——请用 `scripts\update-ui.ps1`（或 `install-local.ps1`）安装到
  `C:\Program Files\Sandboxie-Plus`，那里有完整的运行库。
* 本分发包的 `bin\` 已补齐运行库副本，直接运行 `bin\SandMan.exe` 也能起来，但它只是一个「预览/应急」副本，
  正式入口仍是安装目录里的 SandMan.exe。
* 若已经弹出该错误框：点「确定」关闭即可，SandMan 本体与驱动不受影响。
