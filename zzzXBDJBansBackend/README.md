# zzzXBDJBans Backend

zzzXBDJBans 的 Rust 后端服务。

它同时服务两个调用方：

- Web 管理后台
- CS:GO SourceMod 插件

当前插件侧已经改为 API 通信模式，插件不再直接访问数据库。

## 主要职责

- 管理管理员、封禁、白名单、服务器信息和审计日志
- 管理中断暂停快照和恢复授权
- 提供 Web 后台使用的管理 API
- 提供游戏服插件使用的 `POST /api/plugin/access-check`
- 提供游戏服插件使用的封禁同步和解封同步接口
- 提供游戏服 `interruptpause` 插件使用的中断暂停 API
- 后台异步处理 `player_cache` 和 `player_verifications` 验证任务

## 技术栈

- Rust
- Axum
- SQLx
- PostgreSQL
- Reqwest
- Utoipa

## 关键接口

### Web 侧

- `/api/auth/*`
- `/api/bans/*`
- `/api/whitelist/*`
- `/api/servers/*`
- `/api/logs`
- `/api/interrupt-pause/*`

### 插件侧

- `POST /api/plugin/access-check`
- `POST /api/plugin/ban`
- `POST /api/plugin/unban`
- `POST /api/plugin/interrupt-pause/save`
- `POST /api/plugin/interrupt-pause/peek`
- `POST /api/plugin/interrupt-pause/request-restore`
- `POST /api/plugin/interrupt-pause/fetch-approved`
- `POST /api/plugin/interrupt-pause/complete-restore`
- `POST /api/plugin/interrupt-pause/abort`

插件接口使用表单提交，必须带请求头：

```text
X-Plugin-Token: <PLUGIN_API_TOKEN>
```

请求字段：

- `server_id`
- `steam_id_64`
- `steam_id`
- `player_name`
- `ip_address`

封禁同步接口补充字段：

- `admin_name`
- `admin_steam_id_64`
- `target_name`
- `target_steam_id`
- `target_steam_id_64`
- `target_ip`
- `duration_minutes`
- `reason`

解封同步接口补充字段：

- `admin_name`
- `admin_steam_id_64`
- `target_steam_id`
- `target_steam_id_64`

返回是纯文本键值对，当前格式为：

```text
action=allow|pending|deny
retry_after=2
message=...
```

对于封禁 / 解封同步接口，成功时会返回：

```text
action=banned|unbanned
message=...
ips=...
steam_id=...
steam_id_64=...
steam_id_3=...
```

## Steam 标识规则

当前后端对封禁系统统一采用以下规则：

- `steam_id_64` 是封禁、解封、删除封禁和网站同步的主标识
- Web、插件、控制台传入的 `SteamID2 / SteamID3 / SteamID64` 都会先尝试解析为 `steam_id_64`
- 数据库中仍保留 `steam_id` 和 `steam_id_3` 作为兼容字段，便于老命令和历史数据联动
- 网站端手动封禁、编辑封禁、删除封禁时也会优先走 `steam_id_64`
- 插件端 `sm_ban`、`!ban`、`sm_unban`、`!unban` 同步到后端时也会提交 `target_steam_id_64`

这意味着：

- 新增封禁记录应视 `steam_id_64` 为唯一主账号标识
- 如果输入是 `STEAM_X:Y:Z` 或 `[U:1:Z]`，后端会先转换再处理
- 无法解析成 `steam_id_64` 的目标，封禁同步和网站写入会直接拒绝

## 环境变量

最少需要配置：

```ini
DATABASE_URL=postgres://user:password@localhost:5432/zzzXBDJBans
JWT_SECRET=replace_me
PLUGIN_API_TOKEN=replace_me
SERVER_HOST=0.0.0.0
SERVER_PORT=3000
```

可选配置：

```ini
STEAM_API_KEY=replace_me
PLUGIN_REQUIRED_RATING=3.0
PLUGIN_REQUIRED_LEVEL=1
RUST_LOG=info
```

说明：

- `PLUGIN_API_TOKEN` 用于插件与后端之间的共享鉴权
- `PLUGIN_REQUIRED_RATING` 和 `PLUGIN_REQUIRED_LEVEL` 用于插件进服验证阈值
- `STEAM_API_KEY` 缺失时，后端不会再 panic；依赖 Steam Web API 的资料补全能力会降级

## 本地运行

```bash
cd /home/xbdj/cngokzManagement/zzzXBDJBansBackend
cargo run
```

## 数据库与迁移

迁移目录：

```text
zzzXBDJBansBackend/migrations
```

后端启动时会自动尝试：

- 连接数据库
- 创建数据库
- 执行迁移

即使如此，维护时仍建议先确认：

- `DATABASE_URL` 正确
- PostgreSQL 可连接
- 迁移没有脏状态

## 与插件的关系

当前插件接入流程：

1. 游戏服插件读取 `zzzxbdjbans_server_id`
2. 插件请求 `/api/plugin/access-check`
3. 后端按 `servers.id` 查找服务器配置
4. 后端检查白名单、验证缓存和封禁状态
5. 后端返回 `allow`、`pending` 或 `deny`

当前封禁同步流程：

1. 管理员在游戏内或控制台执行 `!ban` / `!unban` / `sm_ban` / `sm_unban`
2. `zzzXBDJBans.smx` 同步到 `/api/plugin/ban` 或 `/api/plugin/unban`
3. 后端以 `steam_id_64` 为主键写入或更新 `bans`
4. 网站端下发实时封禁时不再调用 `sm_ban` / `sm_unban`
5. 网站只向游戏服发送内部命令 `zzzxbdjbans_sysban` / `zzzxbdjbans_sysunban`

这样做的目的：

- 避免网站手动封禁同时被插件命令监听二次写库
- 避免解封时两条重复记录一起被置为 `unbanned`
- 保持数据库里一条业务动作只对应一条核心封禁记录

这意味着：

- 同一 IP 下多开多个游戏服是支持的
- 关键不是 IP，而是每个游戏服都要配置正确的 `server_id`

## 维护建议

- 修改插件接口返回格式前，必须同步修改 SourceMod 插件解析逻辑
- 修改 `PLUGIN_API_TOKEN` 时，需要同步更新所有游戏服配置
- 如果插件反馈一直 `pending`，先检查 `services/verification_worker.rs`
- 如果多服部署，请优先核对 `servers` 表和每台服的 `server_id`
- 如果遇到网站封禁重复写库，先确认网站侧是否仍在调用 `sm_ban` / `sm_unban`
- 如果遇到玩家已解封但仍无法进服，先确认插件端本地是否已执行 `removeid` 和 `removeip`

## 兼容性检查

本轮静态检查结论：

- `/api/plugin/access-check` 与 `/api/plugin/interrupt-pause/*` 都依赖 `PLUGIN_API_TOKEN` 和有效的 `server_id` 映射，任一缺失都会导致插件侧请求失败。
- `STEAM_API_KEY` 现在是可选项，但缺失时会影响 Steam 资料解析、等级、游戏时长等依赖外部 API 的能力。
- `interruptpause` 插件已切换为纯 HTTP 存储模式，后端必须完成 `interrupt_pause_snapshots` 相关迁移。
- 封禁列表相关查询已新增索引；部署时需要确保最新 migration 已执行，否则优化不会生效。

## 重要源码位置

- 路由入口：`src/main.rs`
- 插件接口：`src/handlers/plugin.rs`
- 服务器管理：`src/handlers/server.rs`
- 封禁逻辑：`src/handlers/ban.rs`
- 白名单逻辑：`src/handlers/whitelist.rs`
- 验证 worker：`src/services/verification_worker.rs`

## 检查命令

开发时最常用：

```bash
cd /home/xbdj/cngokzManagement/zzzXBDJBansBackend
cargo check
```
