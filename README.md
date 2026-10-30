# CrafterBox - Radmin Tools

一个 Windows 桌面工具，围绕 Radmin VPN 做「查看 / 管理 / 探测」，界面用 Tauri 2 + 原生 JS 实现，后端全部 Rust。

> 仅用于学习与研究（Windows 桌面开发、命名管道 IPC、私有协议分析）。与 Famatech 公司无关，不包含其任何代码。

## 功能

**信息 / 账号**
- 本机 Radmin 客户端信息、虚拟网卡与版本号读取
- 多账号配置的保存、导入与切换；切换时**同时恢复** Alias、RID、IPv4（REG_DWORD）、IPv6（REG_BINARY）与 ServerPassword

**LAN → Radmin**
- 直连 Radmin 客户端 GUI 管道，实时读取全部网络与成员（不需要点客户端界面）
- 网络支持多选 + 一键全选，多网络成员自动合并去重
- 表格 8 列：名称 / RID / 虚拟 IP / 真实 IP / 中转 IP / Ping / 通道 / 状态
- **真实 IP** 只显示对端真实公网 IPv4（从会话端点 `0x01e9` 解析；Teredo 隧道地址会解码出内嵌 IPv4，IPv6 直连不混入此列，进悬停提示）
- **中转 IP** 显示中继服务器地址（通道类型 `0x01e8`=3 时）；`0x01c9` 的云端节点地址只进悬停提示
- 一键加入 `Minecraft [中文 1]` ~ `[中文 14]`（首页「加入网络」入口 + 加网表单里的「MC 1-14」按钮）
- 名称搜索、只看在线；全量渲染 + 显隐过滤，上千行不卡
- **点哪格复制哪格**（虚拟 IP 带前缀长度），无需整行复制
- 批量 ICMP 测延迟（24 线程并发，160 个目标上限）
- 直接加入网络（`CJoinNetwork`），返回码可读化（0 成功 / 1012 名称不匹配等）

**LAN → 广播 / 扫描**
- 发送自定义 MOTD 广播包（模板、计数、端口补零等可配）
- 抓取局域网 MOTD 广播，每条结果两行：上行 MOTD、下行对应的 Radmin 昵称（按虚拟 IP 关联快照，未命中显示「未知设备」）

**选项 → 设置 / 日志**
- 持久化配置存于 `%APPDATA%\crafterbox\crafterbox-config.json`
- 运行日志页

## 环境要求

| 项 | 说明 |
|---|---|
| 系统 | Windows 10 1809+ / Windows 11 |
| 运行时 | WebView2 Runtime（Win11 自带） |
| 依赖软件 | 已安装并登录 Radmin VPN 客户端 |
| 构建 | Node.js 18+、Rust stable、MSVC 工具链 |

程序以**管理员权限**运行（内嵌 manifest `requireAdministrator`），因为需要读取 Radmin 的注册表项（服务密码），并通过一个 SYSTEM 服务做桥接。

## 构建

```bash
npm install

# 开发
npm run tauri dev

# 只出 exe，不打安装包
npx tauri build --no-bundle
# 产物：src-tauri/target/release/crafterbox.exe
```

## 目录结构

```
.
├── index.html                 # 单页外壳（自绘标题栏）
├── src/
│   ├── main.js                # 导航与页面懒加载
│   ├── ui.js                  # toast / 对话框
│   ├── styles.css             # 全局样式
│   └── pages/                 # 每个页面导出 render(container)
│       ├── info.js  account.js
│       ├── lan-radmin.js  lan-radmin.css
│       ├── broadcast.js  scan.js
│       └── settings.js  log.js  about.js
└── src-tauri/
    ├── src/
    │   ├── networks.rs        # Radmin GUI 管道协议解析、加网、ICMP
    │   ├── broadcast.rs       # MOTD 广播收发
    │   ├── system_registry.rs # SYSTEM 服务（注册表桥接）
    │   ├── radmin.rs  config.rs  log.rs  win_api.rs  commands.rs
    │   └── main.rs  lib.rs
    ├── tauri.conf.json
    └── windows.manifest       # 提权 + DPI 感知（保持纯 ASCII）
```

## 实现要点（协议部分）

- 通道：`\\.\pipe\RadminVpnGuiChannel`，帧格式 `[u32 LE 总长][payload]`
- 记录（TLV）：`[u32 BE 值长][u8 type][u8 0][u16 BE 字段 ID][值]`
  - `type 0x00` 嵌套消息，`0x01/0x0b` u32 大端，`0x02` u64，`0x03/0x08` UTF-16BE 字符串，`0x09/0x0d` 16 字节（GUID 等）
- 状态转储层级：`0x1320` 根 → `0x1321` 集合 → `0x1323` 网络（内含 `0x1324` 成员槽位）+ `0x1322` 成员明细（内含 `0x1266` 会话）
- 网络与成员靠字段 `0x01e1`（即 RID）关联；`0x0326` 会话状态：0 离线 / 1 TCP / 5 UDP
- 加入网络：消息 ID `3`（`CJoinNetwork`），结果码在字段 `0x0310`
- 延迟测量：`iphlpapi` 的 `IcmpCreateFile` / `IcmpSendEcho`，按 RID 批量并发

## 说明

- 仓库不包含任何抓包数据、账号配置或网络快照。
- 提权清单文件 `src-tauri/windows.manifest` 必须保持**纯 ASCII**：写成其他编码会导致清单解析失败，程序启动时直接报「并行配置不正确」。

## 许可

未附加开源许可证，默认保留所有权利。如需引用或二次分发，请先联系作者。
