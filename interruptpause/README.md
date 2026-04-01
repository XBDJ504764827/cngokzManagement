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
- GOKZ 运行所需 include 和依赖：
  - `gokz/core`
  - `gokz/hud`
  - `movement`
  - `sdktools`
  - `entity_prop_stocks`

### 直接编译

把 `interruptpause/sourcemod/scripting` 放到一套完整的 SourceMod scripting 环境中后执行：

```bash
spcomp gokz-interruptpause.sp
```

如果你的编译环境不在当前目录，保证 include 路径至少覆盖：

```text
interruptpause/sourcemod/scripting
addons/sourcemod/scripting/include
```

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

- 当前仓库环境未提供 `spcomp`，本次未做本机实际编译，只完成了源码整理和前后端联调。
- 插件中仍保留了旧的本地 SQLite 兼容函数，但当前主流程已切换到 HTTP 后端存储。
