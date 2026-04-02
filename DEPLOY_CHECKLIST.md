# Deploy Checklist

这个清单用于上线前和上线后的逐项确认。

## 一、基础环境

- 已准备 Linux 服务器
- 已安装 PostgreSQL
- 已安装 Rust 和 Cargo
- 已安装 Node.js 和 npm
- 已准备 Nginx 或其他反向代理
- 已准备 SourceMod 1.11+ 和 SteamWorks

## 二、数据库

- 已创建数据库 `zzzXBDJBans`
- 已创建独立数据库用户
- 后端使用的数据库用户具备建表和迁移权限
- 已确认数据库可从后端主机访问
- 已配置数据库备份策略

## 三、后端配置

- 已复制 `zzzXBDJBansBackend/.env.example` 为 `.env`
- 已填写真实 `DATABASE_URL`
- 已填写真实 `JWT_SECRET`
- 已填写真实 `PLUGIN_API_TOKEN`
- 已填写真实 `STEAM_API_KEY`
- 已填写 `SERVER_HOST`
- 已填写 `SERVER_PORT`
- 已填写 `BOOTSTRAP_ADMIN_USERNAME`
- 已填写 `BOOTSTRAP_ADMIN_PASSWORD`
- 真实 `.env` 未提交到仓库

## 四、后端构建与启动

- `cargo check` 通过
- `cargo build --release` 通过
- 手动执行后端程序能正常启动
- 启动日志中没有缺少环境变量错误
- 启动日志中没有数据库连接错误
- 启动日志中没有迁移失败错误
- 首次启动后管理员初始化逻辑正常

## 五、后端服务化

- 已创建 `systemd` 服务文件
- `systemctl daemon-reload` 已执行
- 服务已设置开机自启
- 服务当前状态为 `active (running)`
- 服务重启后可正常恢复

## 六、前端配置与构建

- 已安装前端依赖
- `npm run build` 通过
- 已确认 `public/config.json` 中的 `apiBaseUrl`
- 已确认前端静态文件部署目录正确
- 已确认 Nginx `try_files` 支持 SPA 路由
- 已确认 `/api/` 反代到正确的后端地址

## 七、后台首次初始化

- 已能访问前端登录页
- 已能使用 bootstrap 管理员登录
- 已立即修改默认或初始化密码
- 已创建至少一个服务器组
- 已为每个游戏服创建服务器记录
- 已记录每个游戏服的 `servers.id`

## 八、`zzzXBDJBans` 插件

- 已成功编译 `zzzXBDJBans.sp`
- 已将 `.smx` 部署到 `addons/sourcemod/plugins/`
- 已确认游戏服安装 SteamWorks
- 已配置 `zzzxbdjbans_server_id`
- 已配置 `zzzxbdjbans_api_url`
- 已配置 `zzzxbdjbans_api_token`
- 插件 token 与后端 `PLUGIN_API_TOKEN` 一致
- 插件能访问后端 `/api/plugin/access-check`

## 九、`interruptpause` 插件

- 已成功编译 `gokz-interruptpause.sp`
- 已将 `.smx` 部署到 `addons/sourcemod/plugins/`
- 已确认游戏服安装 GOKZ
- 已确认游戏服安装 SteamWorks
- 已配置 `sm_interruptpause_server_id`
- 已配置 `sm_interruptpause_api_base_url`
- 已配置 `sm_interruptpause_api_token`
- 插件 token 与后端 `PLUGIN_API_TOKEN` 一致
- 插件能访问后端 `/api/plugin/interrupt-pause/*`

## 十、联调验证

- 管理员可正常登录后台
- 白名单申请接口可提交
- 白名单列表可加载
- 封禁列表可加载
- 服务器管理页可加载
- 游戏服插件接入后，玩家可收到 `allow`
- 未满足条件玩家可收到 `pending`
- 被封禁玩家可收到 `deny`
- 中断暂停可保存快照
- 中断暂停可提交恢复申请
- 管理后台可审核恢复
- 插件可恢复已批准快照

## 十一、安全确认

- 生产环境使用 HTTPS
- `JWT_SECRET` 为随机强密钥
- `PLUGIN_API_TOKEN` 为随机强密钥
- 数据库未暴露到公网
- 后端未直接裸露高危端口到公网
- 真实密钥未写入仓库
- 日志中未打印敏感信息

## 十二、上线后观察项

- 后端 CPU 和内存正常
- 数据库连接数正常
- 前端页面无明显报错
- 游戏服插件没有持续报超时
- 验证 worker 在持续处理 `player_cache`
- 迁移后的白名单唯一键正常工作
- 中断暂停审核和恢复链路正常
