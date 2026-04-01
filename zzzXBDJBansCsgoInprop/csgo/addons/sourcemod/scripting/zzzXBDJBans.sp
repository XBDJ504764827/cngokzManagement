#include <sourcemod>
#include "SteamWorks.inc"

#pragma semicolon 1
#pragma newdecls required

#define PLUGIN_VERSION "4.0.0"
#define DEFAULT_RETRY_AFTER 2.0

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
ConVar g_cvApiToken;
ConVar g_cvRequestTimeout;

bool g_bRequestPending[MAXPLAYERS + 1];

public void OnPluginStart()
{
    ValidateSteamWorksSupport();

    g_cvServerId = CreateConVar("zzzxbdjbans_server_id", "1", "Server ID for this server instance");
    g_cvApiUrl = CreateConVar("zzzxbdjbans_api_url", "http://127.0.0.1:3000/api/plugin/access-check", "Backend access-check API URL");
    g_cvApiToken = CreateConVar("zzzxbdjbans_api_token", "", "Backend plugin API token");
    g_cvRequestTimeout = CreateConVar("zzzxbdjbans_api_timeout", "10", "HTTP timeout in seconds", _, true, 3.0, true, 30.0);

    AutoExecConfig(true, "zzzXBDJBans");

    CreateTimer(60.0, Timer_CheckBans, _, TIMER_REPEAT);
    LogMessage("zzzXBDJBans Plugin v%s Loaded. Using backend API mode.", PLUGIN_VERSION);
}

public void OnClientDisconnect(int client)
{
    if (client > 0 && client <= MaxClients)
    {
        g_bRequestPending[client] = false;
    }
}

public void OnClientPostAdminCheck(int client)
{
    if (IsFakeClient(client))
    {
        return;
    }

    SendAccessCheck(client, true);
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

    SteamWorks_SetHTTPRequestNetworkActivityTimeout(request, g_cvRequestTimeout.IntValue);
    SteamWorks_SetHTTPRequestContextValue(request, contextValue);
    SteamWorks_SetHTTPRequestHeaderValue(request, "X-Plugin-Token", apiToken);
    SteamWorks_SetHTTPRequestHeaderValue(request, "Content-Type", "application/x-www-form-urlencoded");
    SteamWorks_SetHTTPRequestGetOrPostParameter(request, "server_id", serverId);
    SteamWorks_SetHTTPRequestGetOrPostParameter(request, "steam_id_64", steamId64);
    SteamWorks_SetHTTPRequestGetOrPostParameter(request, "steam_id", steamId);
    SteamWorks_SetHTTPRequestGetOrPostParameter(request, "player_name", playerName);
    SteamWorks_SetHTTPRequestGetOrPostParameter(request, "ip_address", ip);
    SteamWorks_SetHTTPCallbacks(request, OnAccessCheckCompleted);

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
        HandleAccessFailure(client, strict, "验证服务暂时不可用");
        return 0;
    }

    if (view_as<int>(statusCode) != 200)
    {
        LogError("Access check returned HTTP %d for %N. Body: %s", view_as<int>(statusCode), client, body);
        HandleAccessFailure(client, strict, "验证服务返回异常");
        return 0;
    }

    char action[32];
    char message[256];
    int retryAfter;

    ParseApiResponse(body, action, sizeof(action), message, sizeof(message), retryAfter);

    if (StrEqual(action, "allow"))
    {
        return 0;
    }

    if (StrEqual(action, "pending"))
    {
        if (strict)
        {
            if (retryAfter <= 0)
            {
                retryAfter = RoundToFloor(DEFAULT_RETRY_AFTER);
            }
            CreateTimer(float(retryAfter), Timer_RetryAccessCheck, userid);
        }
        return 0;
    }

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
    if (strict && client > 0 && IsClientInGame(client))
    {
        KickClient(client, "验证服务不可用：%s", reason);
    }
    else if (client > 0)
    {
        LogError("Access re-check failed for %N: %s", client, reason);
    }
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

public Action Timer_CheckBans(Handle timer)
{
    for (int i = 1; i <= MaxClients; i++)
    {
        if (IsClientInGame(i) && !IsFakeClient(i))
        {
            SendAccessCheck(i, false);
        }
    }

    return Plugin_Continue;
}
