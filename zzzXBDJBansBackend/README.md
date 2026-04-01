# zzzXBDJBans Backend

zzzXBDJBans 的 Rust 后端服务。

它同时服务两个调用方：

- Web 管理后台
- CS:GO SourceMod 插件

当前插件侧已经改为 API 通信模式，插件不再直接访问数据库。

## 主要职责

- 管理管理员、封禁、白名单、服务器信息和审计日志
- 提供 Web 后台使用的管理 API
- 提供游戏服插件使用的 `POST /api/plugin/access-check`
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

### 插件侧

- `POST /api/plugin/access-check`

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

返回是纯文本键值对，当前格式为：

```text
action=allow|pending|deny
retry_after=2
message=...
```

## 环境变量

最少需要配置：

```ini
DATABASE_URL=postgres://user:password@localhost:5432/zzzXBDJBans
JWT_SECRET=replace_me
STEAM_API_KEY=replace_me
PLUGIN_API_TOKEN=replace_me
SERVER_HOST=0.0.0.0
SERVER_PORT=3000
```

可选配置：

```ini
PLUGIN_REQUIRED_RATING=3.0
PLUGIN_REQUIRED_LEVEL=1
RUST_LOG=info
```

说明：

- `PLUGIN_API_TOKEN` 用于插件与后端之间的共享鉴权
- `PLUGIN_REQUIRED_RATING` 和 `PLUGIN_REQUIRED_LEVEL` 用于插件进服验证阈值

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

这意味着：

- 同一 IP 下多开多个游戏服是支持的
- 关键不是 IP，而是每个游戏服都要配置正确的 `server_id`

## 维护建议

- 修改插件接口返回格式前，必须同步修改 SourceMod 插件解析逻辑
- 修改 `PLUGIN_API_TOKEN` 时，需要同步更新所有游戏服配置
- 如果插件反馈一直 `pending`，先检查 `services/verification_worker.rs`
- 如果多服部署，请优先核对 `servers` 表和每台服的 `server_id`

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
