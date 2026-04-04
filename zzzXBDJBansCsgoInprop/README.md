# zzzXBDJBans CSGO Plugin

zzzXBDJBans 的 CS:GO SourceMod 插件。

当前版本已经不再直接连接数据库。插件只负责把玩家和服务器信息发给后端 API，由后端统一完成白名单、验证缓存、GOKZ 阈值判定和封禁检查。

## 当前架构

- 插件通过 HTTP 调用后端 `POST /api/plugin/access-check`
- 插件通过 HTTP 调用后端 `POST /api/plugin/ban` 和 `POST /api/plugin/unban`
- 后端通过 `server_id` 区分具体游戏服，不依赖游戏服 IP 唯一
- 同一台物理机上多开多个端口没有问题，但每个服都必须配置不同的 `zzzxbdjbans_server_id`
- 插件只在玩家进服时发起检查；若后端返回 `pending`，再按 `retry_after` 做有限重试
- 网站端主动下发游戏服操作时，插件只接收内部命令 `zzzxbdjbans_sysban` / `zzzxbdjbans_sysunban`

## 运行依赖

- MetaMod:Source
- SourceMod 1.11+
- SteamWorks 扩展

注意：
- 没有 `SteamWorks` 扩展时，插件会在加载时失败
- 插件已经不再使用 `configs/databases.cfg`
- 当前版本已移除 60 秒全服轮询，不会再对在线玩家做周期性全量 `access-check`

## 封禁与解封行为

当前插件支持：

- 聊天命令
  - `!ban <玩家steamid/steamid64/#userid> <分钟|0> <理由>`
  - `!unban <玩家steamid/steamid64/#userid>`
- 管理员命令
  - `sm_ban <steamid|steamid64|#userid> <分钟|0> [理由]`
  - `sm_unban <steamid|steamid64|ip>`

当前实现规则：

- `0` 表示永久封禁
- 插件会把 `SteamID2 / SteamID3 / SteamID64` 统一提交给后端，由后端转换成 `steam_id_64`
- 游戏内触发的封禁默认按 IP 封禁
- 游戏内触发的解封默认会做 IP 解封
- 解封时插件会额外执行 `removeid "<steamid>"` 和 `removeip "<steamid>"`，避免历史残留导致玩家仍无法进服
- 网站端手动封禁不会再走 `sm_ban` / `sm_unban`，而是走内部命令，避免重复写入数据库

内部命令：

- `zzzxbdjbans_sysban`
- `zzzxbdjbans_sysunban`

这两个命令只给网站和后端同步链路使用，不建议人工直接操作。

## 配置项

插件启动后会生成 `cfg/sourcemod/zzzXBDJBans.cfg`。

| ConVar | 默认值 | 说明 |
| :--- | :--- | :--- |
| `zzzxbdjbans_server_id` | `1` | 当前游戏服对应的后端 `servers.id` |
| `zzzxbdjbans_api_url` | `http://127.0.0.1:3000/api/plugin/access-check` | 后端插件鉴权接口地址 |
| `zzzxbdjbans_ban_api_url` | `http://127.0.0.1:3000/api/plugin/ban` | 后端封禁同步接口地址 |
| `zzzxbdjbans_unban_api_url` | `http://127.0.0.1:3000/api/plugin/unban` | 后端解封同步接口地址 |
| `zzzxbdjbans_api_token` | 空 | 与后端 `PLUGIN_API_TOKEN` 一致的共享令牌 |
| `zzzxbdjbans_api_timeout` | `10` | HTTP 超时秒数 |
| `zzzxbdjbans_fail_open` | `1` | 后端故障时是否默认放行玩家 |
| `zzzxbdjbans_failure_retry_limit` | `2` | 后端失败后最多重试多少次 |
| `zzzxbdjbans_retry_max_delay` | `30` | 重试等待上限秒数 |

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
sm_cvar zzzxbdjbans_ban_api_url "http://127.0.0.1:3000/api/plugin/ban"
sm_cvar zzzxbdjbans_unban_api_url "http://127.0.0.1:3000/api/plugin/unban"
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
/home/xbdj/cngokzManagement/sourcemod-1.11.0-git6970-linux/addons/sourcemod/scripting/spcomp \
  /home/xbdj/cngokzManagement/zzzXBDJBansCsgoInprop/csgo/addons/sourcemod/scripting/zzzXBDJBans.sp \
  -i/home/xbdj/cngokzManagement/zzzXBDJBansCsgoInprop/csgo/addons/sourcemod/scripting \
  -o/home/xbdj/cngokzManagement/zzzXBDJBansCsgoInprop/csgo/addons/sourcemod/plugins/zzzXBDJBans.smx
```

当前仓库已用上述命令做过一次实编译，结果通过，并且输出已直接覆盖：

```text
csgo/addons/sourcemod/plugins/zzzXBDJBans.smx
```

## 维护要点

- 修改后端地址只需要改 `zzzxbdjbans_api_url`，不需要重新编译
- 修改令牌时，后端 `PLUGIN_API_TOKEN` 和插件 `zzzxbdjbans_api_token` 必须同时更新
- 若玩家进服一直处于验证中，优先检查后端 worker 是否正常处理 `player_cache`
- 若插件加载失败，优先检查 `SteamWorks` 扩展是否已安装
- 若 `sm_ban` 在网站中没生成记录，先检查 `zzzxbdjbans_ban_api_url`
- 若 `sm_unban` 后玩家仍无法进服，先检查服务器端是否已执行 `removeid` / `removeip`
- 若网站端出现重复封禁记录，先检查网站是否仍在对外下发 `sm_ban` / `sm_unban`

## 兼容性检查

本轮静态检查结论：

- 插件使用 `#pragma newdecls required`，应以 SourceMod 1.11+ 为最低编译目标。
- 插件只依赖 `SteamWorks.inc` 提供的 HTTP natives，不依赖数据库配置或额外本地存储。
- 本机 `spcomp` 已实编译通过，当前源码在给定编译器环境下可生成 `.smx`。
- 插件当前已接入封禁和解封同步接口，命令监听与网站内部命令已拆分，避免重复写库。
- 访问控制不再有后台 60 秒轮询；如果需要让新的封禁立即对在线玩家生效，仍需配合重连、管理员手动处理或额外事件触发。
- `pending` 状态仍会按后端 `retry_after` 重试，因此后端验证 worker 异常时，玩家可能长期停留在待验证状态。

## 源码位置

- 插件源码：`csgo/addons/sourcemod/scripting/zzzXBDJBans.sp`
- 最小 SteamWorks include：`csgo/addons/sourcemod/scripting/SteamWorks.inc`
- 编译产物：`csgo/addons/sourcemod/plugins/zzzXBDJBans.smx`
