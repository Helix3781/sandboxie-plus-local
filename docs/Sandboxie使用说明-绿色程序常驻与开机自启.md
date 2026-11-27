# Sandboxie-Plus 本地使用说明：绿色程序常驻沙盒 + 开机自启

适用：本机 `C:\Program Files\Sandboxie-Plus`（v1.19.1 本地改造版）
所有结论都核对过本机安装的 `SbieSettings.ini`（设置项说明）与 SandMan 界面源码，不是凭印象。

---

## 0. 先纠正一个概念

**没有「导入压缩包」这个操作。** Sandboxie 不是虚拟机、不做应用打包安装。

它的模型只有两条：

1. **文件虚拟化**：程序在箱里跑时，**读**走真实磁盘，**写**被重定向到箱根目录（`drive\C\...`、`user\current\...`）。
2. **怎么让程序进箱**：要么你手动「在箱里运行」，要么用「强制」规则让它在哪儿被双击都自动进箱。

所以「把绿色版程序永久配置在沙盒中运行」= **给它配一条强制规则 + 一个运行菜单入口**。程序文件在哪都行，不必搬进箱子。

---

## 1. 绿色程序常驻沙盒

### 1.1 推荐做法：强制目录（Force Folder）

假设绿色程序在 `D:\Green\Foo\foo.exe`：

1. SandMan 主界面 → 右键目标沙盒 → **沙盒选项 / Sandbox Options**
2. 切到 **强制 / Force** 页 → 子页 **强制程序 / Force Programs**
3. 点 **Force Folder**（强制目录）按钮 → 选 `D:\Green\Foo`
4. 确定

之后**无论从哪里**（桌面快捷方式、资源管理器双击、其他程序调用）启动 `D:\Green\Foo\` 下的任何 exe，都会被自动放进这个沙盒。
界面上的原文说明是：

> Programs entered here, or programs started from entered locations, will be put in this sandbox automatically, unless they are explicitly started in another sandbox.

三点补充：

- 只想强制某一个 exe，用 **Force Program**（按进程名，如 `foo.exe`），不限路径。
- 只想强制某个程序**拉起的子进程**（父进程本身保持不进箱），用 **Force Children**。
- 强制规则**对已经在运行的进程不生效**，改完要把程序完全退出再启动。

### 1.2 再给一个入口：运行菜单（Run Menu）

强制规则解决「跑起来就在箱里」，但日常还得自己找 exe。加个菜单项更顺手：

1. 同一个「沙盒选项」窗口 → **常规 / General** 页
2. 找到 **运行菜单 / Run Menu** 列表 → 右下 **Add** → 选 `D:\Green\Foo\foo.exe`
3. 可编辑显示名，确定

之后右键沙盒 → **运行 / Run** 里就有这一项，点一下即可。

它写进 ini 的键是 `RunCommand`，语法：

```ini
RunCommand=显示名称|"D:\Green\Foo\foo.exe"
; 还可以分组（子菜单）
RunCommand=绿色工具\Foo|"D:\Green\Foo\foo.exe"
```

### 1.3 把绿色程序导入箱内（`sbie-box import`）

想让程序**只存在于箱内**、以后都从箱内那份运行，用一条命令：

```powershell
sbie-box import AppCompartmentBox "D:\Green\Foo"
```

（`sbie-box.exe` 装在 `C:\Program Files\Sandboxie-Plus\Tools\`；没装的话用分发包里的 `bin\sbie-box.exe`。）

它做三件事：

1. 把整个 `Foo` 文件夹拷进箱子：`<箱根>\drive\D\Green\Foo`。箱内进程看到的路径仍是原来的 `D:\Green\Foo`，所以自带 DLL、配置、相对路径都不受影响；
2. 在箱子的**运行菜单**里加一条 `foo`，点它就从箱内那份启动（路径含空格时自动换成 8.3 短名，如 `D:\PROGRA~1\...`，因为 Sandboxie 的运行菜单不支持带引号的命令）；
3. 输出 JSON 报告：落地路径、文件数、是否覆盖、挂上的菜单名。

参数：

| 参数 | 说明 |
|---|---|
| `--exe 名称.exe` | 指定挂到菜单的可执行文件；文件夹里只有一个 exe 时自动识别 |
| `--name 显示名` | 运行菜单里显示的名字，默认取 exe 文件名 |
| `--no-menu` | 只拷贝，不加运行菜单 |

例：

```powershell
sbie-box import AppCompartmentBox "D:\Green\Foo" --exe Foo.exe --name "我的工具"
```

手动等价做法（便于核对）：右键沙盒 → **浏览内容 / Browse Content** → 把 `Foo` 拷到箱根 `drive\D\Green\Foo` → 在 **常规 / General → Run Menu** 里加一条 `D:\Green\Foo\foo.exe`。

代价：升级程序要重新导入一次；体积算在箱子里，删箱即彻底消失。注意**拷进去那份是箱内的私货**，宿主的 `D:\Green\Foo` 不受影响，也不会被自动同步。

### 1.4 三种做法怎么选

| 做法 | 程序存哪 | 适合 | 代价 |
|---|---|---|---|
| **Force Folder**（推荐） | 宿主 `D:\Green\Foo` | 日常主力用法，升级方便 | 程序对自身目录的写入被重定向进箱，宿主那份保持原样 |
| **Force Program**（按进程名） | 任意位置 | 一个 exe 到处都可能被启动 | 同名 exe 会被一并强制 |
| **搬进箱根 `drive`**（`sbie-box import`） | 只在箱内 | 要彻底隔离、删箱即净 | 升级麻烦、占箱体积 |

### 1.5 绿色程序常见坑

- **程序写自己目录**：正常，写入会落到箱内对应路径，程序自己看到的是「写成功了」。所以它能用，但你在宿主上看不到改动 —— 这是特性不是 bug。
- **程序要管理员权限**：箱内 UAC 会拦。用 `Start.exe /elevate /box:箱名 "路径"` 启动，或在箱选项里调权限模板。
- **程序要联网/连本地服务**：箱默认隔离 loopback，连宿主的 `127.0.0.1:端口` 会失败；需要放行或改用局域网 IP。
- **配置写进注册表**：也会被重定向到箱内注册表，只影响箱内视图。

---

## 2. 沙盒里的程序开机自动运行

### 2.1 先理解一点：沙盒不是常驻服务

沙盒没有「开机即启动」的状态。它是**有程序要进去跑的时候才被拉起来**。
所以「开机自启」的本质是：**开机后有个东西去调 `Start.exe` 把程序和箱一起拉起来**。

### 2.2 做法 A（推荐）：启动文件夹快捷方式

1. `Win + R` → 输入 `shell:startup` → 回车，打开当前用户的启动文件夹
2. 新建快捷方式，目标填：

```bat
"C:\Program Files\Sandboxie-Plus\Start.exe" /box:YourBox /silent "D:\Green\Foo\foo.exe"
```

3. 保存，重启验证

要点：

- `/box:箱名` 指定沙盒（箱名只允许字母数字下划线）
- `/silent` 不弹 Sandboxie 的启动窗口
- 如果程序本身要提权，加 `/elevate`
- 只想「开机就把箱准备好、但不启动程序」，可以省略最后的程序路径 —— 这时靠下面的 `AutoExec` 来跑程序

### 2.3 做法 B：任务计划程序（能延迟、能开机而非登录）

有时启动文件夹太早（网络/依赖没就绪），用任务计划程序更稳：

1. `taskschd.msc` → 创建任务
2. 触发器：**登录时**（或「开机时」，后者需勾「不管用户是否登录都运行」）
3. 操作：程序 `C:\Program Files\Sandboxie-Plus\Start.exe`，参数 `/box:YourBox /silent "D:\Green\Foo\foo.exe"`
4. 可加「延迟 30 秒」，避开开机高峰

### 2.4 做法 C：让「箱启动」连带启动程序（用 StartProgram，别用 AutoExec）

这两个键名字很像，实际行为差别很大，容易踩坑，所以先说结论：

| 键 | 真实行为 | 能否做开机自启 |
|---|---|---|
| `StartProgram` | **每次箱启动时执行**（由箱内 RPCSS 驻留进程拉起），旧名 `StartCommand` 已被它取代 | ✅ 可以 |
| `AutoExec` | **只在箱「首次初始化」时执行一次**，靠箱内注册表 `\software\SandboxAutoExec` 下的记录做标记，第二次以后不再触发 | ❌ 不行 |

> 设置项字典里 `AutoExec` 的说明写的是「when a sandbox starts」，实测并非如此 —— 它的实现（`core/dll/custom.c` 的 `AutoExec()`）挂在「箱首次定制」分支里，配了也只跑一次。本机已实测确认，详见文末「实测记录」。

**正确用法：**

1. 沙盒选项 → **高级 / Advanced** 页 → **触发器 / Triggers** → **Run Command**（On Start 那一项）
2. 填入要随箱启动的程序，例如 `D:\Green\Foo\foo.exe`
3. 也可以用命令行写（必须经 `SbieIni.exe`，普通权限改不了 `C:\Windows\Sandboxie.ini`）：

```bat
"C:\Program Files\Sandboxie-Plus\SbieIni.exe" append YourBox StartProgram "D:\Green\Foo\foo.exe"
```

**注意一个坑**：`Start.exe` 不能「空启箱」—— `"Start.exe" /box:YourBox /silent` 不带程序会直接返回失败（退出码 1）。
箱是「有进程要进去」才被拉起来的，所以必须给它一个程序先启动，`StartProgram` 里的程序才会跟着跑。
换句话说：`StartProgram` 适合「我已经启动主程序了，另外还想顺手带起几个」，不适合当作唯一起点。

### 2.5 顺带：让 SandMan 管理器自己开机启动

SandMan 主窗口 → **全局设置 / Global Settings** → **常规 / General** → 勾 **Start UI with Windows**（随 Windows 启动 UI）。
这只是让管理器界面在，和「箱内程序自启」是两件事，但一起开更省心。

### 2.6 组合建议

- **只要一个程序开机跑起来**：用 2.2 启动文件夹 + 完整命令行（最简单，一条搞定）
- **主程序 + 附带几个辅助程序一起起**：2.2 启动主程序，再用 2.4 的 `StartProgram` 列出要跟随的程序
- **需要延迟、或要以系统身份/不登录也跑**：用 2.3 任务计划程序

一个常见的完整组合（推荐起步配置）：

```
启动文件夹快捷方式：
  "C:\Program Files\Sandboxie-Plus\Start.exe" /box:YourBox /silent "D:\Green\Foo\foo.exe"
沙盒选项 → 强制页：Force Folder = D:\Green\Foo
沙盒选项 → 常规页：Run Menu 加入 D:\Green\Foo\foo.exe
全局设置 → 常规：勾 Start UI with Windows
```

---

## 3. ini 键速查

| 键 | 作用 | 语法/示例 |
|---|---|---|
| `ForceProcess` | 按**进程名**强制进箱，不限路径 | `ForceProcess=foo.exe` |
| `ForceFolder` | 按**目录**强制进箱 | `ForceFolder=D:\Green\Foo`<br>只强制某进程：`ForceFolder=foo.exe,D:\Green\Foo` |
| `ForceChildren` | 只强制指定父进程拉起的子进程 | `ForceChildren=parent.exe`<br>可加 `Priority=n`（0–9999，越小越优先） |
| `RunCommand` | 在箱的「运行」菜单里加入口 | `RunCommand=显示名\|"D:\Green\Foo\foo.exe"` |
| `StartProgram` | **每次箱启动时**运行（替代旧 `StartCommand`） | `StartProgram=D:\Green\Foo\foo.exe` |
| `AutoExec` | ⚠️ **仅箱首次初始化时跑一次**，不是每次启动 | `AutoExec=D:\Green\Foo\foo.exe` |
| `Enabled` | 箱是否启用 | `Enabled=y` |

写这些键的两种方式：

- GUI：沙盒选项 → 强制 / 常规 / 高级 页（推荐）
- 命令行：`SbieIni.exe set|append <箱名> <键> <值>`（必须走它，才有权限写系统 ini）

---

## 4. 命令行速查

`Start.exe`（`C:\Program Files\Sandboxie-Plus\Start.exe`）：

```bat
Start.exe /box:箱名 /silent "程序路径"       :: 在指定箱里静默运行程序（程序路径必须有）
Start.exe /box:箱名 /elevate /silent "程序"  :: 以提权方式在箱内运行
Start.exe /box:箱名 /listpids                :: 列出该箱内进程 PID
Start.exe /box:箱名 /terminate               :: 终止该箱所有进程
Start.exe /terminate_all                     :: 终止所有箱的进程
```

本地附带的 Rust CLI（`sbie-box.exe`，输出 JSON）：

```bat
sbie-box list                                  :: 列出所有箱
sbie-box status 箱名                            :: 箱状态（running / pids）
sbie-box run 箱名 --exe "D:\Green\Foo\foo.exe"  :: 在箱内运行程序
sbie-box stop 箱名                              :: 终止箱内进程
sbie-box snapshots 箱名                         :: 列出快照
sbie-box boot 箱名 --snapshot 2 --exe "..."     :: 还原快照并启动（loginbox 用的就是它）
```

---

## 5. 怎么确认程序真的在箱里跑

1. **看窗口标题**：标题两侧出现 `[#]` 就是箱内进程（如 `[#] AutoClaw [#]`）。
2. **看 SandMan**：主界面展开对应沙盒，进程列表里能看到它。
3. **看落盘位置**：让程序随便写个文件到它自己目录，然后右键沙盒 → 浏览内容，到 `drive\` 下对应路径找 —— 在箱里能找到、宿主上找不到，就说明隔离生效。
4. **命令行**：`sbie-box status 箱名` 看 `running` 和 `pids`。

---

## 6. 官方手册在哪

- 在线文档：<https://sandboxie-plus.github.io/Sandboxie-docs/>
  设置项逐条说明在 **Sandboxie.ini Settings** 一节，可直接搜键名（`ForceFolder`、`AutoExec`）。
- 本机离线版：`C:\Program Files\Sandboxie-Plus\SbieSettings.ini` 就是内置的设置项字典（含每个键的 `Syntax` 与 `Description` 原文），可以直接用记事本搜键名。
- 界面里的 `sbie://docs/...` 链接需要联网打开在线文档；断网时以本文件和 `SbieSettings.ini` 为准。

---

## 7. 实测记录（本机，2026-11-27）

为确认这份说明可用，用 `cmd.exe` 当「绿色程序替身」在 `AppCompartmentBox` 上实跑过一轮：

| 验证项 | 做法 | 结果 |
|---|---|---|
| 强制目录 `ForceFolder` | 把 `cmd.exe` 放到 `D:\kaifa-soft\_build\greentest`，`SbieIni append AppCompartmentBox ForceFolder D:\\kaifa-soft\\_build\\greentest`，然后**直接启动**该 exe（不指定任何箱） | ✅ 自动进箱；写入落在 `<箱根>\drive\D\kaifa-soft\_build\greentest\written.txt`，宿主目录保持干净 |
| 箱启动运行 `StartProgram` | 配置 `StartProgram=...\cmd.exe /c echo ... > startprog.txt`，再用 `sbie-box run AppCompartmentBox --exe C:\\Windows\\System32\\cmd.exe --args "/c exit"` 触发箱启动 | ✅ 箱内生成 `startprog.txt` |
| `AutoExec` | 同样配置后触发箱启动 | ❌ 未执行 —— 读实现（`core/dll/custom.c` 的 `AutoExec()`）确认：它挂在「箱首次定制」分支里，用箱内注册表 `\software\SandboxAutoExec` 做标记，**每箱只跑一次** |
| 空启箱 | `Start.exe /box:AppCompartmentBox /silent`（不带程序） | ❌ 退出码 1，必须给程序路径 |

测试用的 ini 键与文件已全部清除（`SbieIni delete` 删回 `ForceFolder`/`StartProgram`，删掉测试目录），`AppCompartmentBox` 的既有配置未改动。

补充提醒：SandMan 在运行时会持有自己的箱模型并回写 ini，所以**用命令行改 ini 后，尽量在 SandMan 里也确认一眼**，避免被界面状态覆盖。

---

## 8. 本文涉及的文件

| 文件 | 说明 |
|---|---|
| `C:\Program Files\Sandboxie-Plus\Start.exe` | 把程序送进箱的统一入口（本文所有命令行都用它） |
| `C:\Program Files\Sandboxie-Plus\SbieIni.exe` | 唯一有权限写 `C:\Windows\Sandboxie.ini` 的工具 |
| `C:\Program Files\Sandboxie-Plus\SbieSettings.ini` | 内置设置项字典（离线手册本体，可直接搜键名） |
| `C:\Windows\Sandboxie.ini` | 全部箱配置与全局设置 |
| `D:\kaifa-soft\_build\dist\SandboxieLocal-1.19.1\bin\sbie-box.exe` | 本地 Rust CLI（JSON 输出） |
