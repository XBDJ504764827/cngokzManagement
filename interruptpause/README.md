# GOKZ Interrupt Pause

`gokz-interruptpause.sp` 现已整理为基于网站后端 API 的版本，玩家中断暂停数据统一落库到 `zzzXBDJBansBackend`，并通过 Web 后台“中断暂停授权”页审核恢复。

## 目录结构

```text
interruptpause/
├── README.md
└── sourcemod/
    ├── plugins/
    │   └── gokz-interruptpause.smx
    └── scripting/
        ├── SteamWorks.inc
        └── gokz-interruptpause.sp
```

`SteamWorks.inc` 已随插件目录提供，编译时不再依赖仓库其他目录。

当前版本为纯 HTTP 后端模式：

- 不再使用 SQLite 本地存档
- 不再依赖 `dbi` 或 `configs/databases.cfg`
- 所有快照写入、查询、审核和恢复都通过 `zzzXBDJBansBackend` 完成

## 功能流

1. 玩家执行 `!itimep` / `!interruptpause`，插件采集当前 GOKZ 快照。
2. 插件调用网站后端 `POST /api/plugin/interrupt-pause/save` 保存快照。
3. 玩家重新进服后，插件调用 `POST /api/plugin/interrupt-pause/peek` 查询是否有待恢复存档。
4. 玩家在菜单中选择“申请恢复”后，插件调用 `POST /api/plugin/interrupt-pause/request-restore`。
5. 网站管理员在后台“中断暂停授权”页面审核。
6. 审核通过后，插件调用 `POST /api/plugin/interrupt-pause/fetch-approved` 获取完整快照并恢复。
7. 恢复完成后，插件调用 `POST /api/plugin/interrupt-pause/complete-restore` 标记已恢复。
8. 玩家主动放弃恢复时，插件调用 `POST /api/plugin/interrupt-pause/abort` 标记已终止。

## 编译说明

### 前置依赖

- SourceMod 1.11+ 编译器 `spcomp`
- SteamWorks extension
- 标准 `cstrike` include
- GOKZ 运行所需 include 和依赖：
  - `gokz/core`
  - `gokz/hud`
  - `movement`
  - `sdktools`
  - `entity_prop_stocks`

### 直接编译

把 `interruptpause/sourcemod/scripting` 放到一套完整的 SourceMod scripting 环境中后执行：

```bash
/home/xbdj/cngokzManagement/sourcemod-1.11.0-git6970-linux/addons/sourcemod/scripting/spcomp \
  -i/home/xbdj/cngokzManagement/gokz/addons/sourcemod/scripting/include \
  gokz-interruptpause.sp
```

如果你的编译环境不在当前目录，保证 include 路径至少覆盖：

```text
interruptpause/sourcemod/scripting
addons/sourcemod/scripting/include
```

此外还必须提供这些第三方 include：

```text
movement.inc
gokz/core.inc
gokz/hud.inc
```

如果你使用当前仓库里的 GOKZ 目录，上面的 `-i` 参数就可以直接指向现成的 include 路径。

### 发布文件

编译成功后部署：

- `addons/sourcemod/plugins/gokz-interruptpause.smx`
- `addons/sourcemod/scripting/SteamWorks.inc`
  说明：只在源码二次编译时需要，运行时不需要

## 服务器部署

### 1. 安装后端迁移

先在后端目录执行：

```bash
cd /home/xbdj/cngokzManagement/zzzXBDJBansBackend
cargo run
```

后端启动时会自动执行迁移，新增表：

- `interrupt_pause_snapshots`

### 2. 部署 Web 前端

前端需要包含新增的后台页面“中断暂停授权”：

```bash
cd /home/xbdj/cngokzManagement/zzzXBDJBansWeb
npm run build
```

### 3. 部署 SourceMod 插件

将 `gokz-interruptpause.smx` 放到：

```text
csgo/addons/sourcemod/plugins/
```

确保服务器已安装：

- SteamWorks extension
- CS:GO / CS:S 的 `cstrike` 扩展
- GOKZ
- movement

### 4. 插件配置

插件首次加载后会生成：

```text
cfg/sourcemod/gokz-interruptpause.cfg
```

至少需要修改以下 ConVar：

```cfg
sm_interruptpause_debug "0"
sm_interruptpause_server_id "1"
sm_interruptpause_api_base_url "http://127.0.0.1:3000/api/plugin/interrupt-pause"
sm_interruptpause_api_token "replace_with_plugin_token"
sm_interruptpause_api_timeout "10"
```

配置说明：

- `sm_interruptpause_server_id`
  必须与网站后台 `servers.id` 一致。
- `sm_interruptpause_api_base_url`
  指向后端中断暂停插件接口基路径，不是前端地址。
- `sm_interruptpause_api_token`
  必须与后端环境变量 `PLUGIN_API_TOKEN` 相同。
- `sm_interruptpause_api_timeout`
  HTTP 超时时间，单位秒。

## 后端接口说明

基路径：

```text
/api/plugin/interrupt-pause
```

所有插件接口都要求请求头：

```text
X-Plugin-Token: <PLUGIN_API_TOKEN>
Content-Type: application/x-www-form-urlencoded
```

所有插件接口都会附带这些公共字段：

- `server_id`
- `auth_primary`
- `auth_steamid64`
- `auth_steam3`
- `auth_steam2`
- `auth_engine`

### 1. 保存中断快照

```text
POST /api/plugin/interrupt-pause/save
```

额外字段：

- `player_name`
- `ip_address`
- `map_name`
- `mode`
- `course`
- `time_seconds`
- `checkpoint_count`
- `teleport_count`
- `storage_version`
- `payload`

成功返回：

```text
status=stored
message=中断存档已保存
```

### 2. 查询快照状态

```text
POST /api/plugin/interrupt-pause/peek
```

返回字段示例：

```text
status=pending
message=恢复申请审核中
id=12
map_name=kz_example
time_seconds=123.456
checkpoint_count=8
teleport_count=3
mode=0
course=0
reject_reason=
```

状态说明：

- `none`: 无记录
- `pending`: 已申请，待审核
- `approved`: 已授权，可恢复
- `rejected`: 已拒绝

### 3. 提交恢复申请

```text
POST /api/plugin/interrupt-pause/request-restore
```

成功返回：

```text
status=pending
message=恢复申请已提交，请等待管理员审核
```

### 4. 拉取已授权快照

```text
POST /api/plugin/interrupt-pause/fetch-approved
```

通过时直接返回完整快照 payload 纯文本。

未通过时返回：

```text
status=pending|rejected|available
message=...
```

### 5. 标记恢复完成

```text
POST /api/plugin/interrupt-pause/complete-restore
```

### 6. 终止快照

```text
POST /api/plugin/interrupt-pause/abort
```

## 后台管理说明

页面入口：

- 左侧侧边栏
- `中断暂停授权`

功能：

- 查看所有玩家中断暂停记录
- 按状态筛选
- 审核通过恢复申请
- 拒绝恢复申请并填写理由
- 查看服务器、地图、计时、 checkpoint、teleport、审核时间

## 上线检查清单

- 后端 `PLUGIN_API_TOKEN` 已配置。
- 对应游戏服在后台 `服务器管理` 中已创建，且 `server_id` 正确。
- 前端已部署包含 `InterruptPauseManagement` 新页面的版本。
- SourceMod 已安装 SteamWorks extension。
- 插件加载后 `cfg/sourcemod/gokz-interruptpause.cfg` 已生成并填写正确。
- 游戏服到后端网络可达。
- 后端数据库已执行 `interrupt_pause_snapshots` 迁移。

## 已知说明

- 当前仓库已用 `/home/xbdj/cngokzManagement/sourcemod-1.11.0-git6970-linux/addons/sourcemod/scripting/spcomp` 做过实编译检查。
- `interruptpause` 已在本机通过带 `-i/home/xbdj/cngokzManagement/gokz/addons/sourcemod/scripting/include` 的命令完成实编译。
- 插件已移除旧的 SQLite 兼容代码，当前仅支持 HTTP 后端存储模式。

## 兼容性检查

本轮静态检查结论：

- 语法层面使用了 `#pragma newdecls required`，应以 SourceMod 1.11+ 为最低编译目标。
- 插件显式依赖 `SteamWorks_CreateHTTPRequest`、`SteamWorks_SetHTTPCallbacks`、`SteamWorks_SendHTTPRequest`，未安装 SteamWorks 时会直接 `SetFailState`。
- 插件使用 `CS_TEAM_SPECTATOR`，已补充 `#include <cstrike>`，运行环境应为带标准 `cstrike` include 的 CS 系 SourceMod。
- 插件依赖 `movement`、`gokz/core`、`gokz/hud`，不适用于未安装 GOKZ 的通用服。
- 本机已在补齐 GOKZ include 搜索路径后完成真实编译，说明当前源码与现有 GOKZ 头文件兼容。
- 当前实现会在玩家进服后定时刷新待恢复状态，依赖游戏服到后端的出站 HTTP 连通性。
