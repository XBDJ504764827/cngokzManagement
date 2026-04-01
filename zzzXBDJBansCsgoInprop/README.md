# zzzXBDJBans CSGO Plugin

zzzXBDJBans 的 CS:GO SourceMod 插件。

当前版本已经不再直接连接数据库。插件只负责把玩家和服务器信息发给后端 API，由后端统一完成白名单、验证缓存、GOKZ 阈值判定和封禁检查。

## 当前架构

- 插件通过 HTTP 调用后端 `POST /api/plugin/access-check`
- 后端通过 `server_id` 区分具体游戏服，不依赖游戏服 IP 唯一
- 同一台物理机上多开多个端口没有问题，但每个服都必须配置不同的 `zzzxbdjbans_server_id`
- 插件定时复查在线玩家封禁状态

## 运行依赖

- MetaMod:Source
- SourceMod 1.11+
- SteamWorks 扩展

注意：
- 没有 `SteamWorks` 扩展时，插件会在加载时失败
- 插件已经不再使用 `configs/databases.cfg`

## 配置项

插件启动后会生成 `cfg/sourcemod/zzzXBDJBans.cfg`。

| ConVar | 默认值 | 说明 |
| :--- | :--- | :--- |
| `zzzxbdjbans_server_id` | `1` | 当前游戏服对应的后端 `servers.id` |
| `zzzxbdjbans_api_url` | `http://127.0.0.1:3000/api/plugin/access-check` | 后端插件鉴权接口地址 |
| `zzzxbdjbans_api_token` | 空 | 与后端 `PLUGIN_API_TOKEN` 一致的共享令牌 |
| `zzzxbdjbans_api_timeout` | `10` | HTTP 超时秒数 |

## 多服部署

如果一个社区在同一台服务器上开多个游戏服：

- 每个游戏服都配置自己的 `zzzxbdjbans_server_id`
- 每个 `server_id` 都应在后端 `servers` 表中有唯一记录
- 即使多个游戏服共用同一个公网 IP，只要 `server_id` 不同，后端就能分辨

推荐做法：
- 服务器 A `27015` 使用 `zzzxbdjbans_server_id 1`
- 服务器 A `27016` 使用 `zzzxbdjbans_server_id 2`
- 服务器 A `27017` 使用 `zzzxbdjbans_server_id 3`

## 安装

1. 把插件文件放到游戏服：

```text
csgo/addons/sourcemod/plugins/zzzXBDJBans.smx
```

2. 确保游戏服已安装 `SteamWorks` 扩展。

3. 编辑配置文件：

```cfg
sm_cvar zzzxbdjbans_server_id 1
sm_cvar zzzxbdjbans_api_url "http://127.0.0.1:3000/api/plugin/access-check"
sm_cvar zzzxbdjbans_api_token "replace_with_real_token"
```

4. 重载插件或重启服务器。

## 编译

当前项目使用的 `spcomp` 路径：

```text
/home/xbdj/cngokzManagement/sourcemod-1.11.0-git6970-linux/addons/sourcemod/scripting/spcomp
```

编译命令：

```bash
cd /home/xbdj/cngokzManagement/zzzXBDJBansCsgoInprop/csgo/addons/sourcemod/scripting
/home/xbdj/cngokzManagement/sourcemod-1.11.0-git6970-linux/addons/sourcemod/scripting/spcomp zzzXBDJBans.sp
```

编译成功后会生成：

```text
csgo/addons/sourcemod/scripting/zzzXBDJBans.smx
```

部署时请把它复制到：

```text
csgo/addons/sourcemod/plugins/zzzXBDJBans.smx
```

## 维护要点

- 修改后端地址只需要改 `zzzxbdjbans_api_url`，不需要重新编译
- 修改令牌时，后端 `PLUGIN_API_TOKEN` 和插件 `zzzxbdjbans_api_token` 必须同时更新
- 若玩家进服一直处于验证中，优先检查后端 worker 是否正常处理 `player_cache`
- 若插件加载失败，优先检查 `SteamWorks` 扩展是否已安装

## 源码位置

- 插件源码：`csgo/addons/sourcemod/scripting/zzzXBDJBans.sp`
- 最小 SteamWorks include：`csgo/addons/sourcemod/scripting/SteamWorks.inc`
- 编译产物：`csgo/addons/sourcemod/plugins/zzzXBDJBans.smx`
