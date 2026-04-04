#include <sourcemod>
#include "SteamWorks.inc"

#pragma semicolon 1
#pragma newdecls required

#define PLUGIN_VERSION "4.1.1"
#define DEFAULT_RETRY_AFTER 2.0
#define DEFAULT_ACCESS_CHECK_URL "http://127.0.0.1:3000/api/plugin/access-check"
#define DEFAULT_BAN_URL "http://127.0.0.1:3000/api/plugin/ban"
#define DEFAULT_UNBAN_URL "http://127.0.0.1:3000/api/plugin/unban"
#define INTERNAL_BAN_COMMAND "zzzxbdjbans_sysban"
#define INTERNAL_UNBAN_COMMAND "zzzxbdjbans_sysunban"

public Plugin myinfo =
{
    name = "zzzXBDJBans",
    author = "wwq",
    description = "CS:GO Ban System Integration",
    version = PLUGIN_VERSION,
    url = ""
};

ConVar g_cvServerId;
ConVar g_cvApiUrl;
ConVar g_cvBanApiUrl;
ConVar g_cvUnbanApiUrl;
ConVar g_cvApiToken;
ConVar g_cvRequestTimeout;
ConVar g_cvFailOpen;
ConVar g_cvFailureRetryLimit;
ConVar g_cvRetryMaxDelay;

bool g_bRequestPending[MAXPLAYERS + 1];
int g_iPendingRetryAttempts[MAXPLAYERS + 1];
int g_iFailureRetryAttempts[MAXPLAYERS + 1];
bool g_bBanListenerRegistered;
bool g_bUnbanListenerRegistered;

public void OnPluginStart()
{
    ValidateSteamWorksSupport();

    g_cvServerId = CreateConVar("zzzxbdjbans_server_id", "1", "Server ID for this server instance");
    g_cvApiUrl = CreateConVar("zzzxbdjbans_api_url", DEFAULT_ACCESS_CHECK_URL, "Backend access-check API URL");
    g_cvBanApiUrl = CreateConVar("zzzxbdjbans_ban_api_url", DEFAULT_BAN_URL, "Backend ban sync API URL");
    g_cvUnbanApiUrl = CreateConVar("zzzxbdjbans_unban_api_url", DEFAULT_UNBAN_URL, "Backend unban sync API URL");
    g_cvApiToken = CreateConVar("zzzxbdjbans_api_token", "", "Backend plugin API token");
    g_cvRequestTimeout = CreateConVar("zzzxbdjbans_api_timeout", "10", "HTTP timeout in seconds", _, true, 3.0, true, 30.0);
    g_cvFailOpen = CreateConVar("zzzxbdjbans_fail_open", "1", "Allow players to stay connected when the backend or database is temporarily unavailable.");
    g_cvFailureRetryLimit = CreateConVar("zzzxbdjbans_failure_retry_limit", "2", "How many backend failure retries are attempted before enforcing denial.", _, true, 0.0, true, 10.0);
    g_cvRetryMaxDelay = CreateConVar("zzzxbdjbans_retry_max_delay", "30", "Maximum retry delay in seconds for pending/backend failures.", _, true, 2.0, true, 120.0);

    RegServerCmd(INTERNAL_BAN_COMMAND, Command_InternalBan, "Internal XBDJBans local-only IP ban");
    RegServerCmd(INTERNAL_UNBAN_COMMAND, Command_InternalUnban, "Internal XBDJBans local-only unban cleanup");

    bool commandListenerAvailable = GetFeatureStatus(FeatureType_Capability, FEATURECAP_COMMANDLISTENER) == FeatureStatus_Available;

    if (CommandExists("sm_ban"))
    {
        if (commandListenerAvailable)
        {
            g_bBanListenerRegistered = AddCommandListener(CommandListener_Ban, "sm_ban");
        }
    }
    else
    {
        RegAdminCmd("sm_ban", Command_BanFallback, ADMFLAG_BAN, "sm_ban <steamid|steamid64|#userid> <minutes|0> <reason>");
    }

    if (CommandExists("sm_unban"))
    {
        if (commandListenerAvailable)
        {
            g_bUnbanListenerRegistered = AddCommandListener(CommandListener_Unban, "sm_unban");
        }
    }
    else
    {
        RegAdminCmd("sm_unban", Command_UnbanFallback, ADMFLAG_UNBAN, "sm_unban <steamid|steamid64|#userid>");
    }

    AutoExecConfig(true, "zzzXBDJBans");

    LogMessage("zzzXBDJBans Plugin v%s Loaded. Using backend API mode.", PLUGIN_VERSION);
}

public void OnPluginEnd()
{
    if (g_bBanListenerRegistered)
    {
        RemoveCommandListener(CommandListener_Ban, "sm_ban");
    }

    if (g_bUnbanListenerRegistered)
    {
        RemoveCommandListener(CommandListener_Unban, "sm_unban");
    }
}

public void OnClientDisconnect(int client)
{
    if (client > 0 && client <= MaxClients)
    {
        g_bRequestPending[client] = false;
        ResetRetryState(client);
    }
}

public void OnClientPostAdminCheck(int client)
{
    if (IsFakeClient(client))
    {
        return;
    }

    ResetRetryState(client);
    SendAccessCheck(client, true);
}

public Action OnClientSayCommand(int client, const char[] command, const char[] sArgs)
{
    if (client <= 0 || !IsClientInGame(client) || IsFakeClient(client))
    {
        return Plugin_Continue;
    }

    char payload[512];

    if (ExtractChatCommandPayload(sArgs, "!ban", payload, sizeof(payload)))
    {
        if (!CheckCommandAccess(client, "sm_ban", ADMFLAG_BAN, true))
        {
            ReplyToCommand(client, "[XBDJBans] 你没有封禁权限。");
            return Plugin_Handled;
        }

        return HandlePlayerBanTextCommand(client, payload);
    }

    if (ExtractChatCommandPayload(sArgs, "!unban", payload, sizeof(payload)))
    {
        if (!CheckCommandAccess(client, "sm_unban", ADMFLAG_UNBAN, true))
        {
            ReplyToCommand(client, "[XBDJBans] 你没有解封权限。");
            return Plugin_Handled;
        }

        return HandlePlayerUnbanTextCommand(client, payload);
    }

    return Plugin_Continue;
}

public Action CommandListener_Ban(int client, const char[] command, int argc)
{
    if (client <= 0)
    {
        return HandleServerBanCommand(argc);
    }

    if (!CheckCommandAccess(client, "sm_ban", ADMFLAG_BAN, true))
    {
        ReplyToCommand(client, "[XBDJBans] 你没有封禁权限。");
        return Plugin_Handled;
    }

    return HandlePlayerBanCommand(client, argc);
}

public Action CommandListener_Unban(int client, const char[] command, int argc)
{
    if (client <= 0)
    {
        return HandleServerUnbanCommand(argc);
    }

    if (!CheckCommandAccess(client, "sm_unban", ADMFLAG_UNBAN, true))
    {
        ReplyToCommand(client, "[XBDJBans] 你没有解封权限。");
        return Plugin_Handled;
    }

    return HandlePlayerUnbanCommand(client, argc);
}

public Action Command_BanFallback(int client, int args)
{
    if (client > 0)
    {
        return HandlePlayerBanCommand(client, args);
    }

    return HandleServerBanCommand(args);
}

public Action Command_UnbanFallback(int client, int args)
{
    if (client > 0)
    {
        return HandlePlayerUnbanCommand(client, args);
    }

    return HandleServerUnbanCommand(args);
}

Action HandlePlayerBanCommand(int client, int args)
{
    if (args < 3)
    {
        ReplyToCommand(client, "[XBDJBans] 用法: sm_ban <玩家steamid/steamid64/#userid> <分钟|0> <理由>");
        return Plugin_Handled;
    }

    char targetArg[64];
    int minutes;
    char reason[256];
    if (!ParseBanCommandArguments(targetArg, sizeof(targetArg), minutes, reason, sizeof(reason)))
    {
        ReplyToCommand(client, "[XBDJBans] 用法: sm_ban <玩家steamid/steamid64/#userid> <分钟|0> <理由>");
        return Plugin_Handled;
    }

    int target = ResolveConnectedTarget(client, targetArg, true);
    if (target <= 0)
    {
        return Plugin_Handled;
    }

    if (!SendBanSyncRequest(client, target, minutes, reason))
    {
        ReplyToCommand(client, "[XBDJBans] 封禁同步请求发送失败。");
    }

    return Plugin_Handled;
}

Action HandlePlayerBanTextCommand(int client, const char[] payload)
{
    char targetArg[64];
    int minutes;
    char reason[256];
    if (!ParseBanTextArguments(payload, targetArg, sizeof(targetArg), minutes, reason, sizeof(reason)))
    {
        ReplyToCommand(client, "[XBDJBans] 用法: !ban <玩家steamid/steamid64/#userid> <分钟|0> <理由>");
        return Plugin_Handled;
    }

    int target = ResolveConnectedTarget(client, targetArg, true);
    if (target <= 0)
    {
        return Plugin_Handled;
    }

    if (!SendBanSyncRequest(client, target, minutes, reason))
    {
        ReplyToCommand(client, "[XBDJBans] 封禁同步请求发送失败。");
    }

    return Plugin_Handled;
}

Action HandlePlayerUnbanCommand(int client, int args)
{
    if (args < 1)
    {
        ReplyToCommand(client, "[XBDJBans] 用法: sm_unban <玩家steamid/steamid64/#userid>");
        return Plugin_Handled;
    }

    char targetInput[64];
    GetCmdArg(1, targetInput, sizeof(targetInput));
    StripQuotes(targetInput);
    TrimString(targetInput);

    if (targetInput[0] == '\0')
    {
        ReplyToCommand(client, "[XBDJBans] 用法: sm_unban <玩家steamid/steamid64/#userid>");
        return Plugin_Handled;
    }

    char resolvedSteamTarget[64];
    char resolvedSteamTarget64[64];
    if (!ResolveUnbanTarget(client, targetInput, resolvedSteamTarget, sizeof(resolvedSteamTarget), resolvedSteamTarget64, sizeof(resolvedSteamTarget64)))
    {
        return Plugin_Handled;
    }

    if (!SendUnbanSyncRequest(client, resolvedSteamTarget, resolvedSteamTarget64))
    {
        ReplyToCommand(client, "[XBDJBans] 解封同步请求发送失败。");
    }

    return Plugin_Handled;
}

Action HandlePlayerUnbanTextCommand(int client, const char[] payload)
{
    char targetInput[64];
    if (!ParseSingleTextArgument(payload, targetInput, sizeof(targetInput)))
    {
        ReplyToCommand(client, "[XBDJBans] 用法: !unban <玩家steamid/steamid64/#userid>");
        return Plugin_Handled;
    }

    char resolvedSteamTarget[64];
    char resolvedSteamTarget64[64];
    if (!ResolveUnbanTarget(client, targetInput, resolvedSteamTarget, sizeof(resolvedSteamTarget), resolvedSteamTarget64, sizeof(resolvedSteamTarget64)))
    {
        return Plugin_Handled;
    }

    if (!SendUnbanSyncRequest(client, resolvedSteamTarget, resolvedSteamTarget64))
    {
        ReplyToCommand(client, "[XBDJBans] 解封同步请求发送失败。");
    }

    return Plugin_Handled;
}

Action HandleServerBanCommand(int args)
{
    if (args < 2)
    {
        PrintToServer("[XBDJBans] Usage: sm_ban <#userid|steamid|steamid64> <minutes|0> [reason]");
        return Plugin_Handled;
    }

    char targetArg[64];
    int minutes;
    char reason[256];
    if (!ParseBanCommandArguments(targetArg, sizeof(targetArg), minutes, reason, sizeof(reason), false))
    {
        PrintToServer("[XBDJBans] Usage: sm_ban <#userid|steamid|steamid64> <minutes|0> [reason]");
        return Plugin_Handled;
    }

    int target = ResolveServerTarget(targetArg);
    if (target <= 0)
    {
        PrintToServer("[XBDJBans] Target player is not online.");
        return Plugin_Handled;
    }

    if (reason[0] == '\0')
    {
        strcopy(reason, sizeof(reason), "Banned");
    }

    if (!SendBanSyncRequest(0, target, minutes, reason))
    {
        PrintToServer("[XBDJBans] Ban sync request failed.");
        return Plugin_Handled;
    }

    return Plugin_Handled;
}

Action HandleServerUnbanCommand(int args)
{
    if (args < 1)
    {
        PrintToServer("[XBDJBans] Usage: sm_unban <steamid|steamid64|ip>");
        return Plugin_Handled;
    }

    char target[64];
    GetCmdArg(1, target, sizeof(target));
    StripQuotes(target);
    TrimString(target);

    if (target[0] == '\0')
    {
        PrintToServer("[XBDJBans] Usage: sm_unban <steamid|steamid64|ip>");
        return Plugin_Handled;
    }

    if (!SendUnbanSyncRequest(0, target, target))
    {
        PrintToServer("[XBDJBans] Unban sync request failed.");
        return Plugin_Handled;
    }

    return Plugin_Handled;
}

public Action Command_InternalBan(int args)
{
    if (args < 2)
    {
        PrintToServer("[XBDJBans] Usage: %s <#userid|steamid|steamid64> <minutes|0> [reason]", INTERNAL_BAN_COMMAND);
        return Plugin_Handled;
    }

    char targetArg[64];
    int minutes;
    char reason[256];
    if (!ParseBanCommandArguments(targetArg, sizeof(targetArg), minutes, reason, sizeof(reason), false))
    {
        PrintToServer("[XBDJBans] Usage: %s <#userid|steamid|steamid64> <minutes|0> [reason]", INTERNAL_BAN_COMMAND);
        return Plugin_Handled;
    }

    int target = ResolveServerTarget(targetArg);
    if (target <= 0 || !IsClientInGame(target) || IsFakeClient(target))
    {
        PrintToServer("[XBDJBans] Internal ban target is not online.");
        return Plugin_Handled;
    }

    char targetIp[32];
    if (!GetClientIP(target, targetIp, sizeof(targetIp), true))
    {
        PrintToServer("[XBDJBans] Internal ban failed: missing target IP.");
        return Plugin_Handled;
    }

    if (reason[0] == '\0')
    {
        strcopy(reason, sizeof(reason), "Banned");
    }

    if (!ApplyLocalIpBan(GetClientUserId(target), targetIp, minutes, reason, 0))
    {
        PrintToServer("[XBDJBans] Internal local IP ban failed.");
    }

    return Plugin_Handled;
}

public Action Command_InternalUnban(int args)
{
    if (args < 1)
    {
        PrintToServer("[XBDJBans] Usage: %s <primary-steamid> [steamid2] [steamid3] [ips]", INTERNAL_UNBAN_COMMAND);
        return Plugin_Handled;
    }

    char requestedSteamId[64];
    char steamId2[64];
    char steamId3[64];
    char ips[512];

    GetCmdArg(1, requestedSteamId, sizeof(requestedSteamId));
    StripQuotes(requestedSteamId);
    TrimString(requestedSteamId);

    if (args >= 2)
    {
        GetCmdArg(2, steamId2, sizeof(steamId2));
        StripQuotes(steamId2);
        TrimString(steamId2);
    }
    else
    {
        steamId2[0] = '\0';
    }

    if (args >= 3)
    {
        GetCmdArg(3, steamId3, sizeof(steamId3));
        StripQuotes(steamId3);
        TrimString(steamId3);
    }
    else
    {
        steamId3[0] = '\0';
    }

    if (args >= 4)
    {
        GetCmdArg(4, ips, sizeof(ips));
        StripQuotes(ips);
        TrimString(ips);
    }
    else
    {
        ips[0] = '\0';
    }

    if (requestedSteamId[0] == '\0')
    {
        PrintToServer("[XBDJBans] Internal unban failed: missing SteamID.");
        return Plugin_Handled;
    }

    char resolvedSteamId2[64];
    char resolvedSteamId64[64];
    char resolvedSteamId3[64];
    resolvedSteamId2[0] = '\0';
    resolvedSteamId64[0] = '\0';
    resolvedSteamId3[0] = '\0';

    if (IsSteamId64String(requestedSteamId))
    {
        strcopy(resolvedSteamId64, sizeof(resolvedSteamId64), requestedSteamId);
    }
    else if (IsSteamId2String(requestedSteamId))
    {
        strcopy(resolvedSteamId2, sizeof(resolvedSteamId2), requestedSteamId);
    }
    else if (IsSteamId3String(requestedSteamId))
    {
        strcopy(resolvedSteamId3, sizeof(resolvedSteamId3), requestedSteamId);
    }

    if (steamId2[0] != '\0')
    {
        if (IsSteamId64String(steamId2))
        {
            strcopy(resolvedSteamId64, sizeof(resolvedSteamId64), steamId2);
        }
        else if (IsSteamId2String(steamId2))
        {
            strcopy(resolvedSteamId2, sizeof(resolvedSteamId2), steamId2);
        }
        else if (IsSteamId3String(steamId2))
        {
            strcopy(resolvedSteamId3, sizeof(resolvedSteamId3), steamId2);
        }
    }

    if (steamId3[0] != '\0')
    {
        if (IsSteamId64String(steamId3))
        {
            strcopy(resolvedSteamId64, sizeof(resolvedSteamId64), steamId3);
        }
        else if (IsSteamId2String(steamId3))
        {
            strcopy(resolvedSteamId2, sizeof(resolvedSteamId2), steamId3);
        }
        else if (IsSteamId3String(steamId3))
        {
            strcopy(resolvedSteamId3, sizeof(resolvedSteamId3), steamId3);
        }
    }

    int removedCount = 0;
    int failedCount = 0;
    ApplyLocalUnbanCleanup(requestedSteamId, resolvedSteamId2, resolvedSteamId64, resolvedSteamId3, ips, 0, removedCount, failedCount);
    return Plugin_Handled;
}

void ValidateSteamWorksSupport()
{
    if (GetFeatureStatus(FeatureType_Native, "SteamWorks_CreateHTTPRequest") != FeatureStatus_Available
        || GetFeatureStatus(FeatureType_Native, "SteamWorks_SetHTTPCallbacks") != FeatureStatus_Available
        || GetFeatureStatus(FeatureType_Native, "SteamWorks_SendHTTPRequest") != FeatureStatus_Available)
    {
        SetFailState("SteamWorks extension is required for zzzXBDJBans API mode.");
    }
}

bool ExtractChatCommandPayload(const char[] message, const char[] chatCommand, char[] payload, int payloadLen)
{
    payload[0] = '\0';

    char text[512];
    strcopy(text, sizeof(text), message);
    StripQuotes(text);
    TrimString(text);

    int commandLen = strlen(chatCommand);
    if (strncmp(text, chatCommand, commandLen, false) != 0)
    {
        return false;
    }

    char nextChar = text[commandLen];
    if (nextChar != '\0' && nextChar != ' ' && nextChar != '\t')
    {
        return false;
    }

    if (nextChar == '\0')
    {
        return true;
    }

    strcopy(payload, payloadLen, text[commandLen]);
    TrimString(payload);
    return true;
}

bool ParseBanCommandArguments(char[] targetArg, int targetLen, int &minutes, char[] reason, int reasonLen, bool requireReason = true)
{
    char arguments[512];
    GetCmdArgString(arguments, sizeof(arguments));

    int len = BreakString(arguments, targetArg, targetLen);
    if (len == -1)
    {
        return false;
    }

    char minutesArg[16];
    int nextLen = BreakString(arguments[len], minutesArg, sizeof(minutesArg));
    if (nextLen == -1)
    {
        return false;
    }

    len += nextLen;

    minutes = StringToInt(minutesArg);
    if (minutes < 0)
    {
        minutes = 0;
    }

    if (len >= sizeof(arguments))
    {
        reason[0] = '\0';
    }
    else
    {
        strcopy(reason, reasonLen, arguments[len]);
        StripQuotes(reason);
        TrimString(reason);
    }

    if (requireReason && reason[0] == '\0')
    {
        return false;
    }

    return true;
}

bool ParseBanTextArguments(const char[] payload, char[] targetArg, int targetLen, int &minutes, char[] reason, int reasonLen)
{
    char arguments[512];
    strcopy(arguments, sizeof(arguments), payload);
    TrimString(arguments);

    if (arguments[0] == '\0')
    {
        return false;
    }

    int len = BreakString(arguments, targetArg, targetLen);
    if (len == -1)
    {
        return false;
    }

    char minutesArg[16];
    int nextLen = BreakString(arguments[len], minutesArg, sizeof(minutesArg));
    if (nextLen == -1)
    {
        return false;
    }

    len += nextLen;

    minutes = StringToInt(minutesArg);
    if (minutes < 0)
    {
        minutes = 0;
    }

    if (len >= sizeof(arguments))
    {
        reason[0] = '\0';
    }
    else
    {
        strcopy(reason, reasonLen, arguments[len]);
        StripQuotes(reason);
        TrimString(reason);
    }

    return reason[0] != '\0';
}

bool ParseSingleTextArgument(const char[] payload, char[] output, int outputLen)
{
    output[0] = '\0';

    char text[256];
    strcopy(text, sizeof(text), payload);
    StripQuotes(text);
    TrimString(text);

    if (text[0] == '\0')
    {
        return false;
    }

    strcopy(output, outputLen, text);
    return true;
}

int ResolveConnectedTarget(int adminClient, const char[] targetArg, bool enforceTargeting)
{
    int target = 0;

    if (targetArg[0] == '#')
    {
        int userid = ExtractUserIdFromSpecifier(targetArg);
        if (userid > 0)
        {
            target = GetClientOfUserId(userid);
        }
    }
    else if (LooksLikeSteamIdentifier(targetArg))
    {
        target = FindConnectedClientBySteam(targetArg);
    }
    else
    {
        target = FindTarget(adminClient, targetArg, true, false);
        if (target == -1)
        {
            return -1;
        }
    }

    if (target <= 0 || !IsClientInGame(target))
    {
        ReplyToCommand(adminClient, "[XBDJBans] 未找到在线目标玩家。");
        return -1;
    }

    if (IsFakeClient(target))
    {
        ReplyToCommand(adminClient, "[XBDJBans] 不能封禁机器人。");
        return -1;
    }

    if (enforceTargeting && !CanUserTarget(adminClient, target))
    {
        ReplyToCommand(adminClient, "[XBDJBans] 你不能操作该玩家。");
        return -1;
    }

    return target;
}

int ResolveServerTarget(const char[] targetArg)
{
    if (targetArg[0] == '#')
    {
        int userid = ExtractUserIdFromSpecifier(targetArg);
        if (userid > 0)
        {
            return GetClientOfUserId(userid);
        }
    }

    if (LooksLikeSteamIdentifier(targetArg))
    {
        return FindConnectedClientBySteam(targetArg);
    }

    return FindConnectedClientByName(targetArg);
}

bool ResolveUnbanTarget(int client, const char[] targetInput, char[] steamTarget, int steamTargetLen, char[] steamTarget64, int steamTarget64Len)
{
    steamTarget64[0] = '\0';

    if (LooksLikeSteamIdentifier(targetInput))
    {
        strcopy(steamTarget, steamTargetLen, targetInput);
        strcopy(steamTarget64, steamTarget64Len, targetInput);
        return true;
    }

    int target = ResolveConnectedTarget(client, targetInput, false);
    if (target <= 0)
    {
        ReplyToCommand(client, "[XBDJBans] 请提供玩家 SteamID、SteamID64 或在线玩家 #编号。");
        return false;
    }

    if (!GetClientAuthId(target, AuthId_SteamID64, steamTarget64, steamTarget64Len))
    {
        steamTarget64[0] = '\0';
    }

    if (!GetClientAuthId(target, AuthId_Steam2, steamTarget, steamTargetLen))
        {
            ReplyToCommand(client, "[XBDJBans] 无法获取目标玩家的 SteamID。");
            return false;
        }

    return true;
}

bool SendBanSyncRequest(int client, int target, int minutes, const char[] reason)
{
    char apiUrl[256];
    char apiToken[128];
    char serverId[16];
    char adminName[64];
    char adminSteamId64[64];
    char targetName[128];
    char targetSteamId[64];
    char targetSteamId64[64];
    char targetIp[32];
    char durationMinutes[16];

    GetResolvedApiUrl(g_cvBanApiUrl, "/ban", apiUrl, sizeof(apiUrl));
    g_cvApiToken.GetString(apiToken, sizeof(apiToken));
    IntToString(g_cvServerId.IntValue, serverId, sizeof(serverId));

    TrimString(apiUrl);
    TrimString(apiToken);

    if (apiUrl[0] == '\0' || apiToken[0] == '\0')
    {
        return false;
    }

    GetAdminIdentity(client, adminName, sizeof(adminName), adminSteamId64, sizeof(adminSteamId64));

    GetClientName(target, targetName, sizeof(targetName));
    if (!GetClientAuthId(target, AuthId_Steam2, targetSteamId, sizeof(targetSteamId)))
    {
        targetSteamId[0] = '\0';
    }
    if (!GetClientAuthId(target, AuthId_SteamID64, targetSteamId64, sizeof(targetSteamId64)))
    {
        targetSteamId64[0] = '\0';
    }
    if (!GetClientIP(target, targetIp, sizeof(targetIp), true))
    {
        targetIp[0] = '\0';
    }

    if (targetIp[0] == '\0')
    {
        return false;
    }

    IntToString(minutes, durationMinutes, sizeof(durationMinutes));

    Handle request = SteamWorks_CreateHTTPRequest(k_EHTTPMethodPOST, apiUrl);
    if (request == null)
    {
        return false;
    }

    DataPack pack = new DataPack();
    pack.WriteCell(client > 0 ? GetClientUserId(client) : 0);
    pack.WriteCell(GetClientUserId(target));
    pack.WriteCell(minutes);
    pack.WriteString(targetName);
    pack.WriteString(targetIp);
    pack.WriteString(reason);
    pack.Reset();

    SteamWorks_SetHTTPRequestNetworkActivityTimeout(request, g_cvRequestTimeout.IntValue);
    SteamWorks_SetHTTPRequestContextValue(request, pack, 0);
    SteamWorks_SetHTTPRequestHeaderValue(request, "X-Plugin-Token", apiToken);
    SteamWorks_SetHTTPRequestHeaderValue(request, "Content-Type", "application/x-www-form-urlencoded");
    SteamWorks_SetHTTPRequestGetOrPostParameter(request, "server_id", serverId);
    SteamWorks_SetHTTPRequestGetOrPostParameter(request, "admin_name", adminName);
    SteamWorks_SetHTTPRequestGetOrPostParameter(request, "admin_steam_id_64", adminSteamId64);
    SteamWorks_SetHTTPRequestGetOrPostParameter(request, "target_name", targetName);
    SteamWorks_SetHTTPRequestGetOrPostParameter(request, "target_steam_id", targetSteamId);
    SteamWorks_SetHTTPRequestGetOrPostParameter(request, "target_steam_id_64", targetSteamId64);
    SteamWorks_SetHTTPRequestGetOrPostParameter(request, "target_ip", targetIp);
    SteamWorks_SetHTTPRequestGetOrPostParameter(request, "duration_minutes", durationMinutes);
    SteamWorks_SetHTTPRequestGetOrPostParameter(request, "reason", reason);
    SteamWorks_SetHTTPCallbacks(request, OnBanSyncCompleted, INVALID_FUNCTION, INVALID_FUNCTION, GetMyHandle());

    if (!SteamWorks_SendHTTPRequest(request))
    {
        CloseHandle(request);
        delete pack;
        return false;
    }

    ReplyToTarget(client, "[XBDJBans] 正在同步封禁并写入网站...");
    return true;
}

bool SendUnbanSyncRequest(int client, const char[] steamTarget, const char[] steamTarget64)
{
    char apiUrl[256];
    char apiToken[128];
    char serverId[16];
    char adminName[64];
    char adminSteamId64[64];

    GetResolvedApiUrl(g_cvUnbanApiUrl, "/unban", apiUrl, sizeof(apiUrl));
    g_cvApiToken.GetString(apiToken, sizeof(apiToken));
    IntToString(g_cvServerId.IntValue, serverId, sizeof(serverId));

    TrimString(apiUrl);
    TrimString(apiToken);

    if (apiUrl[0] == '\0' || apiToken[0] == '\0')
    {
        return false;
    }

    GetAdminIdentity(client, adminName, sizeof(adminName), adminSteamId64, sizeof(adminSteamId64));

    Handle request = SteamWorks_CreateHTTPRequest(k_EHTTPMethodPOST, apiUrl);
    if (request == null)
    {
        return false;
    }

    DataPack pack = new DataPack();
    pack.WriteCell(client > 0 ? GetClientUserId(client) : 0);
    pack.WriteString(steamTarget);
    pack.Reset();

    SteamWorks_SetHTTPRequestNetworkActivityTimeout(request, g_cvRequestTimeout.IntValue);
    SteamWorks_SetHTTPRequestContextValue(request, pack, 0);
    SteamWorks_SetHTTPRequestHeaderValue(request, "X-Plugin-Token", apiToken);
    SteamWorks_SetHTTPRequestHeaderValue(request, "Content-Type", "application/x-www-form-urlencoded");
    SteamWorks_SetHTTPRequestGetOrPostParameter(request, "server_id", serverId);
    SteamWorks_SetHTTPRequestGetOrPostParameter(request, "admin_name", adminName);
    SteamWorks_SetHTTPRequestGetOrPostParameter(request, "admin_steam_id_64", adminSteamId64);
    SteamWorks_SetHTTPRequestGetOrPostParameter(request, "target_steam_id", steamTarget);
    SteamWorks_SetHTTPRequestGetOrPostParameter(request, "target_steam_id_64", steamTarget64);
    SteamWorks_SetHTTPCallbacks(request, OnUnbanSyncCompleted, INVALID_FUNCTION, INVALID_FUNCTION, GetMyHandle());

    if (!SteamWorks_SendHTTPRequest(request))
    {
        CloseHandle(request);
        delete pack;
        return false;
    }

    ReplyToTarget(client, "[XBDJBans] 正在同步解封并更新网站...");
    return true;
}

void GetResolvedApiUrl(ConVar specificUrlCvar, const char[] fallbackSuffix, char[] output, int outputLen)
{
    char specificUrl[256];
    char accessCheckUrl[256];

    specificUrlCvar.GetString(specificUrl, sizeof(specificUrl));
    g_cvApiUrl.GetString(accessCheckUrl, sizeof(accessCheckUrl));

    TrimString(specificUrl);
    TrimString(accessCheckUrl);

    bool shouldDerive = specificUrl[0] == '\0';

    if (!shouldDerive)
    {
        if (StrEqual(specificUrl, DEFAULT_BAN_URL, false) || StrEqual(specificUrl, DEFAULT_UNBAN_URL, false))
        {
            shouldDerive = !StrEqual(accessCheckUrl, DEFAULT_ACCESS_CHECK_URL, false);
        }
    }

    if (!shouldDerive)
    {
        strcopy(output, outputLen, specificUrl);
        return;
    }

    strcopy(output, outputLen, accessCheckUrl);
    ReplaceString(output, outputLen, "/access-check", fallbackSuffix, false);
}

public int OnBanSyncCompleted(Handle request, bool failure, bool requestSuccessful, EHTTPStatusCode statusCode, any contextData)
{
    DataPack pack = view_as<DataPack>(contextData);
    pack.Reset();

    int adminUserId = pack.ReadCell();
    int targetUserId = pack.ReadCell();
    int minutes = pack.ReadCell();
    char targetName[128];
    char targetIp[32];
    char reason[256];
    pack.ReadString(targetName, sizeof(targetName));
    pack.ReadString(targetIp, sizeof(targetIp));
    pack.ReadString(reason, sizeof(reason));
    delete pack;

    char body[1024];
    body[0] = '\0';
    ReadResponseBody(request, body, sizeof(body));
    CloseHandle(request);

    int adminClient = GetClientOfUserId(adminUserId);

    if (failure || !requestSuccessful || view_as<int>(statusCode) != 200)
    {
        if (adminClient != 0)
        {
            ReplyToCommand(adminClient, "[XBDJBans] 网站封禁同步失败。");
        }
        else
        {
            PrintToServer("[XBDJBans] Website ban sync failed.");
        }
        LogError("Ban sync failed. HTTP=%d Body=%s", view_as<int>(statusCode), body);
        return 0;
    }

    char action[32];
    char message[256];
    int retryAfter;
    ParseApiResponse(body, action, sizeof(action), message, sizeof(message), retryAfter);

    if (!StrEqual(action, "banned"))
    {
        if (message[0] == '\0')
        {
            strcopy(message, sizeof(message), "网站封禁同步失败。");
        }

        if (adminClient != 0)
        {
            ReplyToCommand(adminClient, "[XBDJBans] %s", message);
        }
        else
        {
            PrintToServer("[XBDJBans] %s", message);
            LogError("Ban sync rejected: %s", message);
        }
        return 0;
    }

    int sourceClient = adminClient != 0 ? adminClient : 0;
    if (!ApplyLocalIpBan(targetUserId, targetIp, minutes, reason, sourceClient))
    {
        if (adminClient != 0)
        {
            ReplyToCommand(adminClient, "[XBDJBans] 网站已同步，但本地 IP 封禁失败。");
        }
        else
        {
            PrintToServer("[XBDJBans] Website sync succeeded, but local IP ban failed.");
        }
        LogError("Local IP ban failed after remote sync. Target=%s IP=%s", targetName, targetIp);
        return 0;
    }

    int liveTarget = GetClientOfUserId(targetUserId);
    LogAction(sourceClient, liveTarget != 0 ? liveTarget : -1, "\"%L\" synced IP ban for \"%s\" (minutes \"%d\") (reason \"%s\")", sourceClient, targetName, minutes, reason);

    if (adminClient != 0)
    {
        ReplyToCommand(adminClient, "[XBDJBans] 已封禁 %s，网站已同步。", targetName);
    }
    else
    {
        PrintToServer("[XBDJBans] Banned %s and synced to website.", targetName);
    }

    return 0;
}

public int OnUnbanSyncCompleted(Handle request, bool failure, bool requestSuccessful, EHTTPStatusCode statusCode, any contextData)
{
    DataPack pack = view_as<DataPack>(contextData);
    pack.Reset();

    int adminUserId = pack.ReadCell();
    char targetSteamId[64];
    pack.ReadString(targetSteamId, sizeof(targetSteamId));
    delete pack;

    char body[1024];
    body[0] = '\0';
    ReadResponseBody(request, body, sizeof(body));
    CloseHandle(request);

    int adminClient = GetClientOfUserId(adminUserId);

    if (failure || !requestSuccessful || (view_as<int>(statusCode) != 200 && view_as<int>(statusCode) != 404))
    {
        if (adminClient != 0)
        {
            ReplyToCommand(adminClient, "[XBDJBans] 网站解封同步失败。");
        }
        else
        {
            PrintToServer("[XBDJBans] Website unban sync failed.");
        }
        LogError("Unban sync failed. HTTP=%d Body=%s", view_as<int>(statusCode), body);
        return 0;
    }

    char action[32];
    char message[256];
    int retryAfter;
    ParseApiResponse(body, action, sizeof(action), message, sizeof(message), retryAfter);

    if (!StrEqual(action, "unbanned"))
    {
        if (message[0] == '\0')
        {
            strcopy(message, sizeof(message), "未找到需要解封的记录。");
        }

        if (adminClient != 0)
        {
            ReplyToCommand(adminClient, "[XBDJBans] %s", message);
        }
        else
        {
            PrintToServer("[XBDJBans] %s", message);
        }
        return 0;
    }

    char ips[512];
    char steamId2[64];
    char steamId64[64];
    char steamId3[64];
    ExtractResponseValue(body, "ips=", ips, sizeof(ips));
    ExtractResponseValue(body, "steam_id=", steamId2, sizeof(steamId2));
    ExtractResponseValue(body, "steam_id_64=", steamId64, sizeof(steamId64));
    ExtractResponseValue(body, "steam_id_3=", steamId3, sizeof(steamId3));

    int removedCount = 0;
    int failedCount = 0;
    int sourceClient = adminClient != 0 ? adminClient : 0;
    ApplyLocalUnbanCleanup(targetSteamId, steamId2, steamId64, steamId3, ips, sourceClient, removedCount, failedCount);

    LogAction(sourceClient, -1, "\"%L\" synced IP unban for \"%s\" (removed \"%d\") (failed \"%d\")", sourceClient, targetSteamId, removedCount, failedCount);

    if (adminClient != 0)
    {
        if (failedCount > 0)
        {
            ReplyToCommand(adminClient, "[XBDJBans] 网站已同步，本地成功解封 %d 个 IP，失败 %d 个。", removedCount, failedCount);
        }
        else
        {
            ReplyToCommand(adminClient, "[XBDJBans] 已解封 %s，网站已同步。", targetSteamId);
        }
    }
    else
    {
        if (failedCount > 0)
        {
            PrintToServer("[XBDJBans] Website synced, local unban removed %d entries and failed %d.", removedCount, failedCount);
        }
        else
        {
            PrintToServer("[XBDJBans] Unbanned %s and synced to website.", targetSteamId);
        }
    }

    return 0;
}

void GetAdminIdentity(int client, char[] adminName, int adminNameLen, char[] adminSteamId64, int adminSteamId64Len)
{
    if (client <= 0)
    {
        strcopy(adminName, adminNameLen, "Console");
        adminSteamId64[0] = '\0';
        return;
    }

    GetClientName(client, adminName, adminNameLen);
    if (!GetClientAuthId(client, AuthId_SteamID64, adminSteamId64, adminSteamId64Len))
    {
        adminSteamId64[0] = '\0';
    }
}

void ReplyToTarget(int client, const char[] format, any ...)
{
    char buffer[256];
    VFormat(buffer, sizeof(buffer), format, 3);

    if (client > 0)
    {
        ReplyToCommand(client, "%s", buffer);
    }
    else
    {
        PrintToServer("%s", buffer);
    }
}

bool ApplyLocalIpBan(int targetUserId, const char[] targetIp, int minutes, const char[] reason, int source)
{
    int liveTarget = GetClientOfUserId(targetUserId);
    if (liveTarget != 0 && IsClientInGame(liveTarget) && !IsFakeClient(liveTarget))
    {
        if (BanClient(liveTarget, minutes, BANFLAG_IP, reason, reason, "sm_ban", source))
        {
            return true;
        }
    }

    if (targetIp[0] == '\0')
    {
        return false;
    }

    return BanIdentity(targetIp, minutes, BANFLAG_IP, reason, "sm_ban", source);
}

bool ApplyLocalBan(int target, int minutes, const char[] reason, int source, bool ipOnly)
{
    int flags = ipOnly ? BANFLAG_IP : BANFLAG_AUTO;
    return BanClient(target, minutes, flags, reason, reason, "sm_ban", source);
}

void ApplyLocalIpUnbanList(const char[] ips, int source, int &removedCount, int &failedCount)
{
    if (ips[0] == '\0')
    {
        return;
    }

    char ipList[16][32];
    int count = ExplodeString(ips, ",", ipList, sizeof(ipList), sizeof(ipList[]));

    for (int i = 0; i < count; i++)
    {
        TrimString(ipList[i]);
        if (ipList[i][0] == '\0')
        {
            continue;
        }

        if (RemoveBan(ipList[i], BANFLAG_IP, "sm_unban", source))
        {
            removedCount++;
        }
        else
        {
            failedCount++;
        }
    }
}

void ApplyLocalUnbanCleanup(
    const char[] requestedSteamId,
    const char[] steamId2,
    const char[] steamId64,
    const char[] steamId3,
    const char[] ips,
    int source,
    int &removedCount,
    int &failedCount)
{
    removedCount = 0;
    failedCount = 0;

    char authTargets[4][64];
    int authCount = 0;

    AddUniqueString(authTargets, sizeof(authTargets), sizeof(authTargets[]), authCount, requestedSteamId);
    AddUniqueString(authTargets, sizeof(authTargets), sizeof(authTargets[]), authCount, steamId2);
    AddUniqueString(authTargets, sizeof(authTargets), sizeof(authTargets[]), authCount, steamId64);
    AddUniqueString(authTargets, sizeof(authTargets), sizeof(authTargets[]), authCount, steamId3);

    bool ranWriteId = false;
    bool ranWriteIp = false;
    for (int i = 0; i < authCount; i++)
    {
        if (RemoveBan(authTargets[i], BANFLAG_AUTHID, "sm_unban", source))
        {
            removedCount++;
        }
        else
        {
            failedCount++;
        }

        ServerCommand("removeid \"%s\"", authTargets[i]);
        ServerCommand("removeip \"%s\"", authTargets[i]);
        ranWriteId = true;
        ranWriteIp = true;
    }

    if (ranWriteId)
    {
        ServerCommand("writeid");
    }

    if (ranWriteIp)
    {
        ServerCommand("writeip");
    }

    ApplyLocalIpUnbanList(ips, source, removedCount, failedCount);
    ApplyLegacyRemoveIpCommands(ips);
}

void ApplyLegacyRemoveIpCommands(const char[] ips)
{
    if (ips[0] == '\0')
    {
        return;
    }

    char ipList[16][32];
    int count = ExplodeString(ips, ",", ipList, sizeof(ipList), sizeof(ipList[]));
    bool ranWriteIp = false;

    for (int i = 0; i < count; i++)
    {
        TrimString(ipList[i]);
        if (ipList[i][0] == '\0')
        {
            continue;
        }

        ServerCommand("removeip \"%s\"", ipList[i]);
        ranWriteIp = true;
    }

    if (ranWriteIp)
    {
        ServerCommand("writeip");
    }
}

void AddUniqueString(char output[][64], int maxRows, int rowLen, int &count, const char[] value)
{
    char trimmed[64];
    strcopy(trimmed, sizeof(trimmed), value);
    TrimString(trimmed);

    if (trimmed[0] == '\0' || count >= maxRows)
    {
        return;
    }

    for (int i = 0; i < count; i++)
    {
        if (StrEqual(output[i], trimmed, false))
        {
            return;
        }
    }

    strcopy(output[count], rowLen, trimmed);
    count++;
}

int ExtractUserIdFromSpecifier(const char[] targetArg)
{
    if (targetArg[0] != '#')
    {
        return 0;
    }

    return StringToInt(targetArg[1]);
}

bool LooksLikeSteamIdentifier(const char[] value)
{
    return IsSteamId64String(value) || IsSteamId2String(value) || IsSteamId3String(value);
}

bool IsSteamId64String(const char[] value)
{
    if (strlen(value) != 17)
    {
        return false;
    }

    for (int i = 0; i < 17; i++)
    {
        if (!IsCharNumeric(value[i]))
        {
            return false;
        }
    }

    return true;
}

bool IsSteamId2String(const char[] value)
{
    return StrContains(value, "STEAM_", false) == 0;
}

bool IsSteamId3String(const char[] value)
{
    return value[0] == '[' && StrContains(value, "[U:", false) == 0;
}

bool IsIpAddress(const char[] value)
{
    if (value[0] == '\0')
    {
        return false;
    }

    int dots = 0;
    for (int i = 0; value[i] != '\0'; i++)
    {
        if (value[i] == '.')
        {
            dots++;
            continue;
        }

        if (!IsCharNumeric(value[i]))
        {
            return false;
        }
    }

    return dots == 3;
}

int FindConnectedClientBySteam(const char[] targetSteam)
{
    char steamId64[64];
    char steamId2[64];
    char steamId3[64];

    for (int client = 1; client <= MaxClients; client++)
    {
        if (!IsClientInGame(client) || IsFakeClient(client))
        {
            continue;
        }

        if (GetClientAuthId(client, AuthId_SteamID64, steamId64, sizeof(steamId64))
            && StrEqual(steamId64, targetSteam, false))
        {
            return client;
        }

        if (GetClientAuthId(client, AuthId_Steam2, steamId2, sizeof(steamId2))
            && StrEqual(steamId2, targetSteam, false))
        {
            return client;
        }

        if (GetClientAuthId(client, AuthId_Steam3, steamId3, sizeof(steamId3))
            && StrEqual(steamId3, targetSteam, false))
        {
            return client;
        }
    }

    return 0;
}

int FindConnectedClientByName(const char[] targetName)
{
    char playerName[128];

    for (int client = 1; client <= MaxClients; client++)
    {
        if (!IsClientInGame(client) || IsFakeClient(client))
        {
            continue;
        }

        GetClientName(client, playerName, sizeof(playerName));
        if (StrEqual(playerName, targetName, false))
        {
            return client;
        }
    }

    return 0;
}

void SendAccessCheck(int client, bool strict)
{
    if (!IsClientInGame(client) || IsFakeClient(client) || g_bRequestPending[client])
    {
        return;
    }

    char apiUrl[256];
    char apiToken[128];
    char serverId[16];
    char steamId64[64];
    char steamId[64];
    char playerName[128];
    char ip[32];

    g_cvApiUrl.GetString(apiUrl, sizeof(apiUrl));
    g_cvApiToken.GetString(apiToken, sizeof(apiToken));
    IntToString(g_cvServerId.IntValue, serverId, sizeof(serverId));

    TrimString(apiUrl);
    TrimString(apiToken);

    if (apiUrl[0] == '\0' || apiToken[0] == '\0')
    {
        HandleAccessFailure(client, strict, "API 配置缺失");
        return;
    }

    if (!GetClientAuthId(client, AuthId_SteamID64, steamId64, sizeof(steamId64)))
    {
        HandleAccessFailure(client, strict, "无效的 SteamID64");
        return;
    }

    if (!GetClientAuthId(client, AuthId_Steam2, steamId, sizeof(steamId)))
    {
        steamId[0] = '\0';
    }

    GetClientName(client, playerName, sizeof(playerName));
    GetClientIP(client, ip, sizeof(ip), true);

    Handle request = SteamWorks_CreateHTTPRequest(k_EHTTPMethodPOST, apiUrl);
    if (request == null)
    {
        HandleAccessFailure(client, strict, "无法创建 HTTP 请求");
        return;
    }

    int contextValue = GetClientUserId(client);
    if (!strict)
    {
        contextValue *= -1;
    }

    g_bRequestPending[client] = true;

    SteamWorksHTTPRequestCompleted callback = OnAccessCheckCompleted;

    SteamWorks_SetHTTPRequestNetworkActivityTimeout(request, g_cvRequestTimeout.IntValue);
    SteamWorks_SetHTTPRequestContextValue(request, contextValue, 0);
    SteamWorks_SetHTTPRequestHeaderValue(request, "X-Plugin-Token", apiToken);
    SteamWorks_SetHTTPRequestHeaderValue(request, "Content-Type", "application/x-www-form-urlencoded");
    SteamWorks_SetHTTPRequestGetOrPostParameter(request, "server_id", serverId);
    SteamWorks_SetHTTPRequestGetOrPostParameter(request, "steam_id_64", steamId64);
    SteamWorks_SetHTTPRequestGetOrPostParameter(request, "steam_id", steamId);
    SteamWorks_SetHTTPRequestGetOrPostParameter(request, "player_name", playerName);
    SteamWorks_SetHTTPRequestGetOrPostParameter(request, "ip_address", ip);
    SteamWorks_SetHTTPCallbacks(request, callback, INVALID_FUNCTION, INVALID_FUNCTION, GetMyHandle());

    if (!SteamWorks_SendHTTPRequest(request))
    {
        g_bRequestPending[client] = false;
        CloseHandle(request);
        HandleAccessFailure(client, strict, "发送 HTTP 请求失败");
    }
}

public int OnAccessCheckCompleted(Handle request, bool failure, bool requestSuccessful, EHTTPStatusCode statusCode, any contextData)
{
    int userid = contextData;
    bool strict = true;

    if (userid < 0)
    {
        strict = false;
        userid *= -1;
    }

    int client = GetClientOfUserId(userid);
    if (client == 0)
    {
        CloseHandle(request);
        return 0;
    }

    g_bRequestPending[client] = false;

    char body[1024];
    body[0] = '\0';
    ReadResponseBody(request, body, sizeof(body));
    CloseHandle(request);

    if (failure || !requestSuccessful)
    {
        LogError("Access check HTTP request failed for %N", client);
        g_iPendingRetryAttempts[client] = 0;
        if (strict)
        {
            if (g_cvFailOpen.BoolValue)
            {
                LogMessage("Access check failed open for %N", client);
                ResetRetryState(client);
                return 0;
            }

            if (ScheduleRetry(client, userid, RoundToFloor(DEFAULT_RETRY_AFTER), true))
            {
                return 0;
            }
        }
        HandleAccessFailure(client, strict, "验证服务暂时不可用");
        return 0;
    }

    if (view_as<int>(statusCode) != 200)
    {
        LogError("Access check returned HTTP %d for %N. Body: %s", view_as<int>(statusCode), client, body);
        g_iPendingRetryAttempts[client] = 0;
        if (strict)
        {
            if (g_cvFailOpen.BoolValue)
            {
                LogMessage("Access check returned HTTP %d and failed open for %N", view_as<int>(statusCode), client);
                ResetRetryState(client);
                return 0;
            }

            if (ScheduleRetry(client, userid, RoundToFloor(DEFAULT_RETRY_AFTER), true))
            {
                return 0;
            }
        }
        HandleAccessFailure(client, strict, "验证服务返回异常");
        return 0;
    }

    g_iFailureRetryAttempts[client] = 0;

    char action[32];
    char message[256];
    int retryAfter;

    ParseApiResponse(body, action, sizeof(action), message, sizeof(message), retryAfter);

    if (StrEqual(action, "allow"))
    {
        ResetRetryState(client);
        return 0;
    }

    if (StrEqual(action, "pending"))
    {
        if (strict)
        {
            ScheduleRetry(client, userid, retryAfter, false);
        }
        return 0;
    }

    ResetRetryState(client);

    if (message[0] == '\0')
    {
        strcopy(message, sizeof(message), "访问被拒绝");
    }

    KickClient(client, "%s", message);
    return 0;
}

void ReadResponseBody(Handle request, char[] body, int bodyLen)
{
    int size = 0;
    if (!SteamWorks_GetHTTPResponseBodySize(request, size) || size <= 0)
    {
        body[0] = '\0';
        return;
    }

    if (size >= bodyLen)
    {
        size = bodyLen - 1;
    }

    if (!SteamWorks_GetHTTPResponseBodyData(request, body, size))
    {
        body[0] = '\0';
        return;
    }

    body[size] = '\0';
}

void ParseApiResponse(const char[] body, char[] action, int actionLen, char[] message, int messageLen, int &retryAfter)
{
    action[0] = '\0';
    message[0] = '\0';
    retryAfter = RoundToFloor(DEFAULT_RETRY_AFTER);

    ExtractResponseValue(body, "action=", action, actionLen);
    ExtractResponseValue(body, "message=", message, messageLen);

    char retryBuffer[16];
    ExtractResponseValue(body, "retry_after=", retryBuffer, sizeof(retryBuffer));
    if (retryBuffer[0] != '\0')
    {
        retryAfter = StringToInt(retryBuffer);
    }

    TrimString(action);
    TrimString(message);
}

void ExtractResponseValue(const char[] body, const char[] key, char[] output, int outputLen)
{
    output[0] = '\0';

    int start = StrContains(body, key);
    if (start == -1)
    {
        return;
    }

    start += strlen(key);

    int bodyLen = strlen(body);
    int end = start;
    while (end < bodyLen && body[end] != '\n' && body[end] != '\r')
    {
        end++;
    }

    int copyLen = end - start;
    if (copyLen <= 0)
    {
        return;
    }

    if (copyLen >= outputLen)
    {
        copyLen = outputLen - 1;
    }

    for (int i = 0; i < copyLen; i++)
    {
        output[i] = body[start + i];
    }
    output[copyLen] = '\0';
}

void HandleAccessFailure(int client, bool strict, const char[] reason)
{
    if (client <= 0)
    {
        return;
    }

    if (strict && g_cvFailOpen.BoolValue)
    {
        if (IsClientInGame(client))
        {
            LogError("Access check failed open for %N: %s", client, reason);
        }
        else
        {
            LogError("Access check failed open for client %d: %s", client, reason);
        }
        ResetRetryState(client);
        return;
    }

    if (strict && IsClientInGame(client))
    {
        KickClient(client, "验证服务不可用：%s", reason);
    }
    else
    {
        LogError("Access re-check failed for %N: %s", client, reason);
    }
}

void ResetRetryState(int client)
{
    g_iPendingRetryAttempts[client] = 0;
    g_iFailureRetryAttempts[client] = 0;
}

bool ScheduleRetry(int client, int userid, int suggestedRetryAfter, bool failureRetry)
{
    int baseDelay = suggestedRetryAfter > 0 ? suggestedRetryAfter : RoundToFloor(DEFAULT_RETRY_AFTER);
    int maxDelay = g_cvRetryMaxDelay.IntValue;
    int attempt = 0;

    if (failureRetry)
    {
        if (g_iFailureRetryAttempts[client] >= g_cvFailureRetryLimit.IntValue)
        {
            return false;
        }

        attempt = g_iFailureRetryAttempts[client];
        g_iFailureRetryAttempts[client]++;
    }
    else
    {
        attempt = g_iPendingRetryAttempts[client];
        g_iPendingRetryAttempts[client]++;
    }

    float delay = GetBackoffDelay(baseDelay, attempt, maxDelay);
    CreateTimer(delay, Timer_RetryAccessCheck, userid);
    return true;
}

float GetBackoffDelay(int baseDelay, int attempt, int maxDelay)
{
    int delay = baseDelay;

    for (int i = 0; i < attempt; i++)
    {
        if (delay >= maxDelay)
        {
            break;
        }

        delay *= 2;
        if (delay > maxDelay)
        {
            delay = maxDelay;
        }
    }

    return float(delay);
}

public Action Timer_RetryAccessCheck(Handle timer, any userid)
{
    int client = GetClientOfUserId(userid);
    if (client == 0 || !IsClientInGame(client) || IsFakeClient(client))
    {
        return Plugin_Stop;
    }

    SendAccessCheck(client, true);
    return Plugin_Stop;
}
