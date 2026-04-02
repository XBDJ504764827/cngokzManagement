# cngokzManagement

这是一个面向 CS:GO / KZ 社区的完整管理仓库，包含 Web 管理后台、Rust 后端、进服校验插件和 GOKZ 中断暂停插件。

仓库当前的核心目标是：

- 管理管理员账号、白名单、封禁、服务器和审计日志
- 在玩家进服时由后端统一判定是否允许进入
- 通过 `server_id` 支持一台机器多开多个游戏服
- 把 GOKZ 中断暂停快照统一存到后端，再由管理员审核恢复

## 仓库包含什么

- `zzzXBDJBansBackend`
  Rust 后端，提供 `/api/*` 和插件接口
- `zzzXBDJBansWeb`
  Vue 3 管理后台和公开页面
- `zzzXBDJBansCsgoInprop`
  SourceMod 进服校验插件
- `interruptpause`
  GOKZ 中断暂停插件
- `gokz`
  GOKZ 相关源码和 include，用于插件编译和本地参考
- `sourcemod-1.11.0-git6970-linux`
  SourceMod 编译器环境

## 总体目录结构

```text
cngokzManagement/
├── README.md
├── DEPLOY_CHECKLIST.md
├── zzzXBDJBansBackend/
│   ├── .env.example
│   ├── Cargo.toml
│   ├── migrations/
│   └── src/
├── zzzXBDJBansWeb/
│   ├── public/
│   ├── src/
│   ├── package.json
│   └── vite.config.js
├── zzzXBDJBansCsgoInprop/
├── interruptpause/
├── gokz/
└── sourcemod-1.11.0-git6970-linux/
```

## 系统架构

### Web 管理链路

1. 浏览器访问 `zzzXBDJBansWeb`
2. 前端请求 `zzzXBDJBansBackend` 的 `/api/*`
3. 后端连接 PostgreSQL
4. 管理员在后台完成白名单、封禁、服务器、中断暂停审核等操作

### 进服验证链路

1. 玩家连接游戏服
2. `zzzXBDJBans.smx` 收集玩家 Steam 信息和服务器 `server_id`
3. 插件调用 `POST /api/plugin/access-check`
4. 后端根据白名单、验证缓存、封禁和服务器配置做判定
5. 后端返回 `allow`、`pending` 或 `deny`

### 中断暂停链路

1. 玩家在 GOKZ 中触发中断暂停
2. `gokz-interruptpause.smx` 把快照保存到后端
3. 玩家重连后可申请恢复
4. 管理员在 Web 后台审核
5. 插件拉取已批准快照并执行恢复

## 组件关系图

```text
Browser
  -> zzzXBDJBansWeb
  -> zzzXBDJBansBackend
  -> PostgreSQL

CS:GO Server
  -> zzzXBDJBans.smx
  -> zzzXBDJBansBackend /api/plugin/access-check

CS:GO Server + GOKZ
  -> gokz-interruptpause.smx
  -> zzzXBDJBansBackend /api/plugin/interrupt-pause/*
```

## 环境要求

### 后端

- Rust 稳定版，建议 1.75+
- Cargo
- PostgreSQL 13+
- Linux 开发或部署环境
- 可选：Redis

### 前端

- Node.js 18+
- npm 9+

### SourceMod 插件

- MetaMod:Source
- SourceMod 1.11+
- SteamWorks extension
- `interruptpause` 还需要 GOKZ、movement、cstrike 等依赖

## 快速了解每个子项目

### `zzzXBDJBansBackend`

主要负责：

- 管理员鉴权
- 白名单管理
- 封禁管理
- 服务器管理
- 审计日志
- 进服验证插件接口
- 中断暂停插件接口
- `player_cache` 和 `player_verifications` 异步处理

关键目录：

```text
zzzXBDJBansBackend/
├── .env.example
├── Cargo.toml
├── migrations/
├── src/
│   ├── handlers/
│   ├── models/
│   ├── services/
│   ├── utils/
│   └── main.rs
└── README.md
```

### `zzzXBDJBansWeb`

主要负责：

- 管理员登录
- 白名单管理
- 封禁管理
- 服务器管理
- 审计日志查看
- 中断暂停审核
- 公开封禁页和公开白名单页

关键目录：

```text
zzzXBDJBansWeb/
├── public/
│   └── config.json
├── src/
│   ├── components/
│   ├── composables/
│   ├── layouts/
│   ├── router/
│   └── views/
├── package.json
└── README.md
```

### `zzzXBDJBansCsgoInprop`

主要负责：

- 玩家进服时调用后端 API
- 执行允许进入、等待验证或拒绝进入
- 用 `server_id` 标识具体游戏服

### `interruptpause`

主要负责：

- 保存 GOKZ 中断暂停快照
- 查询待恢复状态
- 提交恢复申请
- 拉取已批准快照并恢复

## 配置总览

### 后端核心环境变量

最少需要：

```ini
DATABASE_URL=postgres://user:password@host:5432/zzzXBDJBans
JWT_SECRET=replace_with_long_random_secret
PLUGIN_API_TOKEN=replace_with_shared_plugin_token
SERVER_HOST=0.0.0.0
SERVER_PORT=8080
```

常用可选项：

```ini
REDIS_URL=redis://127.0.0.1:6379/
STEAM_API_KEY=replace_me
RUST_LOG=info
PLUGIN_REQUIRED_RATING=3.0
PLUGIN_REQUIRED_LEVEL=1
BOOTSTRAP_ADMIN_USERNAME=admin
BOOTSTRAP_ADMIN_PASSWORD=change_me
VERIFICATION_BATCH_SIZE=20
VERIFICATION_FETCH_CONCURRENCY=10
VERIFICATION_WORKER_ACTIVE_SLEEP_MS=750
VERIFICATION_WORKER_IDLE_SLEEP_MS=3000
GLOBAL_BAN_CACHE_TTL_SECONDS=300
GLOBAL_BAN_FETCH_CONCURRENCY=8
```

关键说明：

- `JWT_SECRET` 必填，否则后端启动时会 panic
- `PLUGIN_API_TOKEN` 必须和两个插件中的 token 一致
- `STEAM_API_KEY` 缺失时，Steam 等级、游戏时长和资料补全会降级
- `BOOTSTRAP_ADMIN_*` 用于数据库中没有管理员时初始化首个超管

### 前端配置

前端支持两种配置来源：

1. 构建时 `.env.development` / `.env.production`
2. 运行时 `public/config.json`

运行时配置格式：

```json
{
  "apiBaseUrl": "http://your-backend-host:8080/api"
}
```

建议：

- 开发环境可直接改 `.env.development`
- 生产环境优先改 `public/config.json`

### `zzzXBDJBans` 插件配置

插件加载后会生成：

```text
cfg/sourcemod/zzzXBDJBans.cfg
```

至少需要设置：

```cfg
sm_cvar zzzxbdjbans_server_id 1
sm_cvar zzzxbdjbans_api_url "http://127.0.0.1:8080/api/plugin/access-check"
sm_cvar zzzxbdjbans_api_token "replace_with_plugin_token"
sm_cvar zzzxbdjbans_api_timeout "10"
```

### `interruptpause` 插件配置

参考示例文件：

```text
interruptpause/sourcemod/cfg/sourcemod/gokz-interruptpause.cfg.example
```

至少需要设置：

```cfg
sm_interruptpause_debug "0"
sm_interruptpause_server_id "1"
sm_interruptpause_api_base_url "http://127.0.0.1:8080/api/plugin/interrupt-pause"
sm_interruptpause_api_token "replace_with_plugin_token"
sm_interruptpause_api_timeout "10"
```

## 纯本地开发版流程

这一节面向：

- 在同一台开发机上启动前端和后端
- 先不部署到公网
- 可以暂时不接真实游戏服，只验证管理后台和后端 API

### 本地开发目标

完成以下事情：

1. 本地 PostgreSQL 可用
2. 后端可以连库并完成迁移
3. 前端可以连接后端
4. 管理员可以登录后台
5. 能在后台看到服务器、白名单、封禁等页面

### 第 1 步：准备 PostgreSQL

安装 PostgreSQL 后，创建数据库和账号。

示例：

```sql
CREATE USER "zzzXBDJBans" WITH PASSWORD 'zzzXBDJBans';
CREATE DATABASE "zzzXBDJBans" OWNER "zzzXBDJBans";
GRANT ALL PRIVILEGES ON DATABASE "zzzXBDJBans" TO "zzzXBDJBans";
```

如果你本地已经有数据库，也可以直接复用。

### 第 2 步：配置后端 `.env`

进入后端目录：

```bash
cd /home/xbdj/cngokzManagement/zzzXBDJBansBackend
```

复制示例配置：

```bash
cp .env.example .env
```

把至少这几项改成你本地真实值：

```ini
DATABASE_URL=postgres://zzzXBDJBans:zzzXBDJBans@127.0.0.1:5432/zzzXBDJBans
JWT_SECRET=your_local_secret
PLUGIN_API_TOKEN=your_local_plugin_token
SERVER_HOST=0.0.0.0
SERVER_PORT=8080
```

如果你要初始化首个超管，建议在本地加上：

```ini
BOOTSTRAP_ADMIN_USERNAME=admin
BOOTSTRAP_ADMIN_PASSWORD=change_me_now
```

### 第 3 步：检查后端能否编译

```bash
cd /home/xbdj/cngokzManagement/zzzXBDJBansBackend
cargo check
```

通过后再继续。

### 第 4 步：启动后端

```bash
cd /home/xbdj/cngokzManagement/zzzXBDJBansBackend
cargo run
```

启动时后端会做这些事情：

1. 读取 `.env`
2. 检查 `JWT_SECRET`
3. 连接数据库
4. 执行 `migrations/` 下的迁移
5. 在管理员为空时根据 `BOOTSTRAP_ADMIN_*` 创建超管
6. 启动验证 worker 和其他后台任务

常见问题：

- 报 `JWT_SECRET must be set`
  说明 `.env` 缺少 `JWT_SECRET`
- 报数据库连接失败
  检查 `DATABASE_URL`
- 报迁移失败
  检查数据库权限和迁移状态

### 第 5 步：配置前端开发环境

进入前端目录：

```bash
cd /home/xbdj/cngokzManagement/zzzXBDJBansWeb
```

安装依赖：

```bash
npm install
```

确认 `.env.development` 里的 API 地址指向本地后端：

```ini
VITE_API_BASE_URL=http://127.0.0.1:8080/api
```

也可以直接改 `public/config.json`，但开发时一般优先用 `.env.development`。

### 第 6 步：启动前端

```bash
cd /home/xbdj/cngokzManagement/zzzXBDJBansWeb
npm run dev
```

默认地址：

- 前端：`http://127.0.0.1:5173`
- 后端：`http://127.0.0.1:8080`

### 第 7 步：首次登录后台

如果你在后端 `.env` 中设置了：

```ini
BOOTSTRAP_ADMIN_USERNAME=admin
BOOTSTRAP_ADMIN_PASSWORD=change_me_now
```

那么首次登录可直接使用这组账号。

登录后建议立即修改密码。

### 第 8 步：初始化基础数据

本地开发推荐先做这几件事：

1. 进入服务器管理页面
2. 创建至少一个服务器组
3. 在组里创建至少一个服务器
4. 记住这台服务器的 `id`

原因：

- `zzzXBDJBans` 插件和 `interruptpause` 插件都依赖 `servers.id`
- 不先建服务器，后续插件接入时无法正确识别游戏服

### 第 9 步：验证核心页面

建议按下面顺序点一遍：

1. 登录页
2. 社区组管理
3. 白名单管理
4. 封禁管理
5. 管理员管理
6. 审计日志
7. 中断暂停审核
8. 公开白名单页
9. 公开封禁页

### 第 10 步：本地 API 自检

你至少应该确认：

- `/api/auth/login` 可用
- `/api/auth/me` 可用
- `/api/whitelist/apply` 可用
- `/api/bans/public` 可用

### 第 11 步：如果要本地联调插件

先启动后端，再准备本地或测试用 CS:GO 服务器。

你需要：

- 在后台先建服务器，得到 `server_id`
- 把 `PLUGIN_API_TOKEN` 配到插件
- 确认插件的 API 地址指向本地后端

#### 编译 `zzzXBDJBans`

```bash
cd /home/xbdj/cngokzManagement/zzzXBDJBansCsgoInprop/csgo/addons/sourcemod/scripting
/home/xbdj/cngokzManagement/sourcemod-1.11.0-git6970-linux/addons/sourcemod/scripting/spcomp zzzXBDJBans.sp
```

#### 编译 `interruptpause`

```bash
cd /home/xbdj/cngokzManagement/interruptpause/sourcemod/scripting
/home/xbdj/cngokzManagement/sourcemod-1.11.0-git6970-linux/addons/sourcemod/scripting/spcomp \
  -i/home/xbdj/cngokzManagement/gokz/addons/sourcemod/scripting/include \
  gokz-interruptpause.sp
```

#### 本地联调时插件配置重点

- `server_id` 必须和后台里的服务器记录一致
- token 必须和后端 `.env` 中的 `PLUGIN_API_TOKEN` 一致
- `api_url` 或 `api_base_url` 必须能从游戏服访问到本机后端

### 第 12 步：本地开发常用命令

后端：

```bash
cd zzzXBDJBansBackend
cargo check
cargo run
```

前端：

```bash
cd zzzXBDJBansWeb
npm install
npm run dev
npm run build
```

插件编译：

```bash
cd zzzXBDJBansCsgoInprop/csgo/addons/sourcemod/scripting
/home/xbdj/cngokzManagement/sourcemod-1.11.0-git6970-linux/addons/sourcemod/scripting/spcomp zzzXBDJBans.sp
```

```bash
cd interruptpause/sourcemod/scripting
/home/xbdj/cngokzManagement/sourcemod-1.11.0-git6970-linux/addons/sourcemod/scripting/spcomp \
  -i/home/xbdj/cngokzManagement/gokz/addons/sourcemod/scripting/include \
  gokz-interruptpause.sp
```

## 生产环境部署版流程

这一节面向：

- 后端、前端和数据库分离部署
- 前端走静态文件托管
- 后端用 `systemd` 或类似服务管理
- 一个或多个游戏服连接后端

### 推荐生产拓扑

```text
Internet
  -> Nginx
     -> zzzXBDJBansWeb dist
     -> zzzXBDJBansBackend
        -> PostgreSQL

Game Server 1
  -> zzzXBDJBans.smx
  -> gokz-interruptpause.smx
  -> Backend

Game Server 2
  -> zzzXBDJBans.smx
  -> gokz-interruptpause.smx
  -> Backend
```

### 第 1 步：准备服务器规划

生产环境建议至少明确以下信息：

- 后端部署主机 IP / 域名
- PostgreSQL 主机地址
- 前端访问域名
- 每个游戏服的名称、IP、端口
- 每个游戏服计划绑定的 `server_id`

### 第 2 步：部署 PostgreSQL

要求：

- 网络可从后端机器访问
- 用户有建表和迁移权限
- 数据库字符集和时区配置合理

建议：

- 单独创建业务用户
- 不要直接使用 `postgres` 超级用户跑应用
- 打开自动备份

### 第 3 步：准备后端配置

进入生产后端目录后，新建 `.env`：

```bash
cd /opt/cngokzManagement/zzzXBDJBansBackend
cp .env.example .env
```

至少填写：

```ini
DATABASE_URL=postgres://zzzXBDJBans:strong_password@db-host:5432/zzzXBDJBans
JWT_SECRET=very_long_random_secret
PLUGIN_API_TOKEN=very_long_random_plugin_token
SERVER_HOST=127.0.0.1
SERVER_PORT=8080
STEAM_API_KEY=replace_me
RUST_LOG=info
BOOTSTRAP_ADMIN_USERNAME=admin
BOOTSTRAP_ADMIN_PASSWORD=change_me_now
```

生产建议：

- `JWT_SECRET` 至少 32 字节随机字符串
- `PLUGIN_API_TOKEN` 独立随机生成，不要和其他密码复用
- 后端监听 `127.0.0.1`，由 Nginx 对外暴露
- 不要把真实 `.env` 提交到 Git

### 第 4 步：构建后端

```bash
cd /opt/cngokzManagement/zzzXBDJBansBackend
cargo build --release
```

产物通常是：

```text
target/release/zzzXBDJBansBackend
```

### 第 5 步：首次启动后端

直接先手动跑一次：

```bash
cd /opt/cngokzManagement/zzzXBDJBansBackend
./target/release/zzzXBDJBansBackend
```

首次启动要确认：

- 能成功读取 `.env`
- 能成功连接数据库
- 能完成所有迁移
- 没有缺少关键环境变量
- 如果管理员为空，能成功创建 bootstrap 超管

确认没有问题后再放到 `systemd`。

### 第 6 步：配置后端为 `systemd` 服务

示例：

```ini
[Unit]
Description=zzzXBDJBans Backend
After=network.target postgresql.service

[Service]
Type=simple
WorkingDirectory=/opt/cngokzManagement/zzzXBDJBansBackend
ExecStart=/opt/cngokzManagement/zzzXBDJBansBackend/target/release/zzzXBDJBansBackend
Restart=always
RestartSec=3
User=www-data
Group=www-data

[Install]
WantedBy=multi-user.target
```

启用：

```bash
sudo systemctl daemon-reload
sudo systemctl enable zzzxbdjbans-backend
sudo systemctl start zzzxbdjbans-backend
sudo systemctl status zzzxbdjbans-backend
```

### 第 7 步：构建前端

```bash
cd /opt/cngokzManagement/zzzXBDJBansWeb
npm install
npm run build
```

构建产物在：

```text
zzzXBDJBansWeb/dist/
```

### 第 8 步：配置前端运行时地址

生产环境推荐修改：

```text
zzzXBDJBansWeb/public/config.json
```

示例：

```json
{
  "apiBaseUrl": "https://admin.example.com/api"
}
```

如果你使用同域反代，也可以写：

```json
{
  "apiBaseUrl": "/api"
}
```

### 第 9 步：部署前端到 Nginx

示例：

```nginx
server {
    listen 80;
    server_name admin.example.com;

    root /opt/cngokzManagement/zzzXBDJBansWeb/dist;
    index index.html;

    location / {
        try_files $uri $uri/ /index.html;
    }

    location /api/ {
        proxy_pass http://127.0.0.1:8080/api/;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    }
}
```

如果使用 HTTPS，请正常配置证书。

### 第 10 步：登录后台并初始化业务数据

生产环境第一次进入后台后，先完成：

1. 修改 bootstrap 管理员密码
2. 创建服务器组
3. 为每个游戏服创建服务器记录
4. 记录每台游戏服对应的 `servers.id`

这一点非常关键，因为插件最终依赖 `servers.id` 区分不同游戏服。

### 第 11 步：部署 `zzzXBDJBans` 插件

先编译：

```bash
cd /opt/cngokzManagement/zzzXBDJBansCsgoInprop/csgo/addons/sourcemod/scripting
/opt/cngokzManagement/sourcemod-1.11.0-git6970-linux/addons/sourcemod/scripting/spcomp zzzXBDJBans.sp
```

把编译后的 `.smx` 部署到游戏服：

```text
csgo/addons/sourcemod/plugins/zzzXBDJBans.smx
```

然后配置：

```cfg
sm_cvar zzzxbdjbans_server_id 1
sm_cvar zzzxbdjbans_api_url "https://admin.example.com/api/plugin/access-check"
sm_cvar zzzxbdjbans_api_token "same_as_backend_plugin_api_token"
sm_cvar zzzxbdjbans_api_timeout "10"
```

每个游戏服都要：

- 使用不同的 `server_id`
- 指向同一个后端 API
- 使用同一个 `PLUGIN_API_TOKEN`

### 第 12 步：部署 `interruptpause` 插件

先编译：

```bash
cd /opt/cngokzManagement/interruptpause/sourcemod/scripting
/opt/cngokzManagement/sourcemod-1.11.0-git6970-linux/addons/sourcemod/scripting/spcomp \
  -i/opt/cngokzManagement/gokz/addons/sourcemod/scripting/include \
  gokz-interruptpause.sp
```

部署：

```text
csgo/addons/sourcemod/plugins/gokz-interruptpause.smx
```

配置：

```cfg
sm_interruptpause_debug "0"
sm_interruptpause_server_id "1"
sm_interruptpause_api_base_url "https://admin.example.com/api/plugin/interrupt-pause"
sm_interruptpause_api_token "same_as_backend_plugin_api_token"
sm_interruptpause_api_timeout "10"
```

### 第 13 步：生产联调顺序

建议严格按下面顺序检查：

1. 后端服务正常启动
2. 前端页面能打开
3. 管理员可以登录
4. 后台能创建服务器记录
5. 白名单申请可以提交
6. `zzzXBDJBans` 插件可以访问 `/api/plugin/access-check`
7. 玩家能收到 `allow` / `pending` / `deny`
8. 中断暂停插件可以保存快照
9. 后台可以审核快照
10. 插件可以恢复已批准快照

### 第 14 步：生产环境更新流程

#### 更新后端

```bash
cd /opt/cngokzManagement/zzzXBDJBansBackend
git pull
cargo build --release
sudo systemctl restart zzzxbdjbans-backend
```

确认：

- 服务正常启动
- 新迁移执行成功
- 日志无异常

#### 更新前端

```bash
cd /opt/cngokzManagement/zzzXBDJBansWeb
git pull
npm install
npm run build
```

然后刷新 Nginx 静态目录。

#### 更新插件

重新编译 `.sp`，替换服务器上的 `.smx`，重载插件或重启游戏服。

## 每个项目如何编译

### 后端

```bash
cd zzzXBDJBansBackend
cargo check
cargo build
cargo build --release
```

### 前端

```bash
cd zzzXBDJBansWeb
npm install
npm run dev
npm run build
```

### `zzzXBDJBans` 插件

```bash
cd zzzXBDJBansCsgoInprop/csgo/addons/sourcemod/scripting
/home/xbdj/cngokzManagement/sourcemod-1.11.0-git6970-linux/addons/sourcemod/scripting/spcomp zzzXBDJBans.sp
```

### `interruptpause` 插件

```bash
cd interruptpause/sourcemod/scripting
/home/xbdj/cngokzManagement/sourcemod-1.11.0-git6970-linux/addons/sourcemod/scripting/spcomp \
  -i/home/xbdj/cngokzManagement/gokz/addons/sourcemod/scripting/include \
  gokz-interruptpause.sp
```

## 每个项目如何部署

### 后端部署结果

你最终需要：

- 可执行程序
- 真实 `.env`
- PostgreSQL 连接
- `systemd` 或其他守护进程管理
- 反向代理

### 前端部署结果

你最终需要：

- `dist/` 静态文件
- 正确的 `config.json`
- Nginx / Apache / CDN

### 插件部署结果

你最终需要：

- 编译好的 `.smx`
- 正确的 `server_id`
- 正确的插件 token
- 服务器上安装 SteamWorks

## 数据和识别规则

### 白名单识别

当前白名单以 `steam_id_64` 作为唯一识别依据。

这意味着：

- 不再依赖 `steam_id` 做唯一约束
- 玩家使用不同 SteamID 表示法时，后端会统一解析到 `steam_id_64`

### 多服部署规则

支持同一台机器多开多个服。

前提是：

- 每个服在后台有独立 `servers` 记录
- 每个服配置独立 `server_id`
- 插件不要共用错误的 `server_id`

## 安全建议

- 不要把真实 `.env` 提交到仓库
- `JWT_SECRET` 和 `PLUGIN_API_TOKEN` 使用随机长字符串
- 生产环境建议 HTTPS
- 生产环境建议数据库定期备份
- 不建议让后端直接监听公网 `0.0.0.0:8080`
- 建议通过 Nginx 暴露统一域名

## 常见问题

### 后端启动时提示 `JWT_SECRET must be set`

说明：

- 后端 `.env` 缺少 `JWT_SECRET`

处理：

- 复制 `zzzXBDJBansBackend/.env.example`
- 在 `.env` 中填写真实值
- 重新启动

### 插件提示 token 无效

检查三处是否完全一致：

- 后端 `PLUGIN_API_TOKEN`
- `zzzxbdjbans_api_token`
- `sm_interruptpause_api_token`

### 玩家一直处于 `pending`

优先检查：

- 后端是否在线
- `STEAM_API_KEY` 是否正确
- 验证 worker 是否在运行
- 外部 Steam / GOKZ API 是否可访问

### 同一台机器多个服识别错乱

通常原因：

- 多个服使用了相同的 `server_id`
- 后台 `servers` 记录配置不对应

### 中断暂停无法恢复

优先检查：

- 后端迁移是否已执行
- `interrupt_pause_snapshots` 表是否存在
- 插件 `api_base_url` 是否正确
- `server_id` 是否正确
- 后台是否已审核通过

## 相关文档

独立子项目文档：

- `zzzXBDJBansBackend/README.md`
- `zzzXBDJBansWeb/README.md`
- `zzzXBDJBansCsgoInprop/README.md`
- `interruptpause/README.md`

部署前建议同时查看：

- `DEPLOY_CHECKLIST.md`
- `zzzXBDJBansBackend/.env.example`
