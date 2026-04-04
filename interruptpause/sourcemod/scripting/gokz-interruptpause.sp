#pragma semicolon 1
#pragma newdecls required

#include <sourcemod>
#include <cstrike>
#include <sdktools>
#include <entity_prop_stocks>
#include "SteamWorks.inc"
#include <movement>
#include <gokz/core>
#include <gokz/hud>

#define PLUGIN_NAME "GOKZ Interrupt Pause"
#define STORAGE_VERSION 1
#define SNAPSHOT_IP_MAX_LENGTH 64
#define SNAPSHOT_PAYLOAD_MAX 32768
#define RUN_MONITOR_INTERVAL_SECONDS 1.0
#define PERIODIC_SNAPSHOT_INTERVAL_SECONDS 30.0
#define AIRBORNE_DISCONNECT_PENALTY_SECONDS 15.0
#define INTERRUPT_API_URL_MAX 256
#define INTERRUPT_API_TOKEN_MAX 128
#define INTERRUPT_RESPONSE_MAX 32768
#define INTERRUPT_REFRESH_INTERVAL_SECONDS 15.0

enum InterruptRestoreState
{
	InterruptRestoreState_None = 0,
	InterruptRestoreState_Pending,
	InterruptRestoreState_Approved,
	InterruptRestoreState_Rejected
}

enum InterruptSaveRequestKind
{
	InterruptSaveRequestKind_Auto = 0,
	InterruptSaveRequestKind_Manual
}

ConVar gCV_InterruptPauseDebug;
ConVar gCV_InterruptPauseServerId;
ConVar gCV_InterruptPauseApiBaseUrl;
ConVar gCV_InterruptPauseApiToken;
ConVar gCV_InterruptPauseApiTimeout;
Handle gH_RunMonitorTimer = null;
int gI_DuckSpeedBaseOffset = -1;
bool gB_DuckSpeedBaseOffsetCached;
bool gB_CanRestoreThisConnection[MAXPLAYERS + 1];
bool gB_AutoInterruptSaveOnDisconnect[MAXPLAYERS + 1];
bool gB_HasPendingInterrupt[MAXPLAYERS + 1];
bool gB_CanShowPendingInterruptMenu[MAXPLAYERS + 1];
bool gB_PendingInterruptMenuDisplayed[MAXPLAYERS + 1];
bool gB_ManualInterruptSaveInFlight[MAXPLAYERS + 1];
bool gB_HasSafeSnapshot[MAXPLAYERS + 1];
float gF_PendingInterruptTime[MAXPLAYERS + 1];
float gF_NextPeriodicSnapshotAt[MAXPLAYERS + 1];
char gS_PendingInterruptMap[MAXPLAYERS + 1][PLATFORM_MAX_PATH];
char gS_PendingInterruptRejectReason[MAXPLAYERS + 1][192];
bool gB_PendingInterruptMapMatches[MAXPLAYERS + 1];
bool gB_PendingInterruptLookupInFlight[MAXPLAYERS + 1];
bool gB_PendingInterruptRestoreFetchInFlight[MAXPLAYERS + 1];
InterruptRestoreState gI_PendingInterruptRestoreState[MAXPLAYERS + 1];
int gI_LastObservedCheckpointCount[MAXPLAYERS + 1];
Handle gH_PendingInterruptMenuTimer[MAXPLAYERS + 1];
Handle gH_PendingInterruptRefreshTimer[MAXPLAYERS + 1];

public Plugin myinfo =
{
	name = PLUGIN_NAME,
	author = "wqq",
	description = "Save and restore a paused GOKZ run across reconnects.",
	version = "1.4.4",
	url = ""
};

enum struct InterruptSnapshot
{
	char auth[MAX_AUTHID_LENGTH];
	char savedIp[SNAPSHOT_IP_MAX_LENGTH];
	char map[PLATFORM_MAX_PATH];
	int mode;
	int course;
	float time;
	float origin[3];
	float angles[3];
	int groundEnt;
	int flags;
	float velocity[3];
	float duckAmount;
	bool ducking;
	bool ducked;
	float lastDuckTime;
	float duckSpeed;
	float stamina;
	MoveType movetype;
	float ladderNormal[3];
	int collisionGroup;
	float waterJumpTime;
	bool hasWalkMovedSinceLastJump;
	float ignoreLadderJumpTimeOffset;
	float lastPositionAtFullCrouchSpeed[2];
	int checkpointVersion;
	int checkpointCount;
	int teleportCount;
	ArrayList checkpointData;
	ArrayList undoTeleportData;
	bool hasUndoTeleportData;
	bool restoredFromSafePosition;
	bool penalizedForAirDisconnect;
	bool exists;
}

InterruptSnapshot gSafeSnapshots[MAXPLAYERS + 1];

public void OnPluginStart()
{
	gCV_InterruptPauseDebug = CreateConVar("sm_interruptpause_debug", "0", "Enable interruptpause debug logging.", FCVAR_NONE, true, 0.0, true, 1.0);
	ValidateSteamWorksSupport();
	gCV_InterruptPauseServerId = CreateConVar("sm_interruptpause_server_id", "1", "Server ID used by interruptpause backend APIs.");
	gCV_InterruptPauseApiBaseUrl = CreateConVar("sm_interruptpause_api_base_url", "http://127.0.0.1:3000/api/plugin/interrupt-pause", "Base URL for interruptpause backend API.");
	gCV_InterruptPauseApiToken = CreateConVar("sm_interruptpause_api_token", "", "Plugin token for interruptpause backend API.");
	gCV_InterruptPauseApiTimeout = CreateConVar("sm_interruptpause_api_timeout", "10", "Interruptpause HTTP timeout in seconds.", _, true, 3.0, true, 30.0);
	AutoExecConfig(true, "gokz-interruptpause");
	HookEvent("player_disconnect", Event_PlayerDisconnect, EventHookMode_Pre);
	HookEvent("player_team", Event_PlayerTeam, EventHookMode_Post);
	HookEvent("player_spawn", Event_PlayerSpawn, EventHookMode_Post);
	gH_RunMonitorTimer = CreateTimer(RUN_MONITOR_INTERVAL_SECONDS, Timer_RunMonitor, _, TIMER_REPEAT | TIMER_FLAG_NO_MAPCHANGE);
	RegConsoleCmd("sm_itimep", Command_InterruptPause, "[KZ] Save your paused GOKZ run for later restoration.");
	RegConsoleCmd("sm_interruptpause", Command_InterruptPause, "[KZ] Save your paused GOKZ run for later restoration.");
	RegConsoleCmd("sm_itimepstart", Command_InterruptPauseStart, "[KZ] Enable auto-save on disconnect for the current run.");
}

public void OnClientPutInServer(int client)
{
	gB_CanRestoreThisConnection[client] = true;
	gB_AutoInterruptSaveOnDisconnect[client] = false;
	gB_CanShowPendingInterruptMenu[client] = false;
	gB_PendingInterruptMenuDisplayed[client] = false;
	gB_ManualInterruptSaveInFlight[client] = false;
	ResetRunMonitorState(client);
	ResetPendingInterruptState(client);
	SchedulePendingInterruptRefresh(client, 0.5);
}

public void OnClientAuthorized(int client, const char[] auth)
{
	if (client <= 0 || client > MaxClients || !IsClientInGame(client) || IsFakeClient(client))
	{
		return;
	}

	SchedulePendingInterruptRefresh(client, 0.1);
}

public void OnClientDisconnect(int client)
{
	gB_CanRestoreThisConnection[client] = false;
	gB_ManualInterruptSaveInFlight[client] = false;
	ResetRunMonitorState(client);
	ResetPendingInterruptState(client);
}

public void OnPluginEnd()
{
	gH_RunMonitorTimer = null;

	for (int client = 1; client <= MaxClients; client++)
	{
		ResetRunMonitorState(client);
	}
}

void ValidateSteamWorksSupport()
{
	if (GetFeatureStatus(FeatureType_Native, "SteamWorks_CreateHTTPRequest") != FeatureStatus_Available
		|| GetFeatureStatus(FeatureType_Native, "SteamWorks_SetHTTPCallbacks") != FeatureStatus_Available
		|| GetFeatureStatus(FeatureType_Native, "SteamWorks_SendHTTPRequest") != FeatureStatus_Available)
	{
		SetFailState("SteamWorks extension is required for interruptpause backend API mode.");
	}
}

public Action Command_InterruptPause(int client, int args)
{
	if (!CanUsePlayerCommand(client))
	{
		return Plugin_Handled;
	}

	if (gB_ManualInterruptSaveInFlight[client])
	{
		ReplyToCommand(client, "[InterruptPause] 中断保存请求仍在处理中，请稍候再试。");
		return Plugin_Handled;
	}

	char reason[192];
	if (!GetInterruptSaveEligibilityFailure(client, reason, sizeof(reason)))
	{
		ReplyToCommand(client, "[InterruptPause] %s", reason);
		return Plugin_Handled;
	}

	InterruptSnapshot snapshot;
	if (!BuildSnapshotForSave(client, false, snapshot))
	{
		DebugLog("save capture failed for client=%N", client);
		CleanupSnapshot(snapshot);
		ReplyToCommand(client, "[InterruptPause] 无法读取你的 GOKZ 进度。");
		return Plugin_Handled;
	}

	bool wrote = WriteSnapshot(client, snapshot, InterruptSaveRequestKind_Manual);
	CleanupSnapshot(snapshot);
	if (!wrote)
	{
		DebugLog("save write failed for client=%N auth=%s", client, snapshot.auth);
		ReplyToCommand(client, "[InterruptPause] 保存失败，无法写入后端中断存档。");
		return Plugin_Handled;
	}

	ReplyToCommand(client, "[InterruptPause] 正在保存中断进度，请稍候...");
	return Plugin_Handled;
}

public Action Command_InterruptPauseStart(int client, int args)
{
	if (!CanUsePlayerCommand(client))
	{
		return Plugin_Handled;
	}

	if (!ShouldMonitorClientRun(client))
	{
		ReplyToCommand(client, "[InterruptPause] 你需要先处于有效且未暂停的 GOKZ 计时中，自动中断保存才会生效。");
		return Plugin_Handled;
	}

	ActivateRunMonitorState(client);
	ReplyToCommand(client, "[InterruptPause] 自动监控保存现在默认开启。只要你处于有效计时且时间大于 0，插件就会自动周期保存，并在断线时做最后一次快照。");
	return Plugin_Handled;
}

public void Event_PlayerDisconnect(Event event, const char[] name, bool dontBroadcast)
{
	int client = GetClientOfUserId(event.GetInt("userid"));
	if (client <= 0 || client > MaxClients)
	{
		return;
	}

	TryAutoSaveSnapshotOnDisconnect(client);
}

public void Event_PlayerTeam(Event event, const char[] name, bool dontBroadcast)
{
	int client = GetClientOfUserId(event.GetInt("userid"));
	if (client <= 0 || client > MaxClients)
	{
		return;
	}

	if (event.GetInt("team") <= CS_TEAM_SPECTATOR)
	{
		gB_CanShowPendingInterruptMenu[client] = false;
	}
}

public void Event_PlayerSpawn(Event event, const char[] name, bool dontBroadcast)
{
	int client = GetClientOfUserId(event.GetInt("userid"));
	if (client <= 0 || client > MaxClients || !IsClientInGame(client) || IsFakeClient(client))
	{
		return;
	}

	if (GetClientTeam(client) <= CS_TEAM_SPECTATOR)
	{
		return;
	}

	gB_CanShowPendingInterruptMenu[client] = true;

	if (!gB_HasPendingInterrupt[client])
	{
		SchedulePendingInterruptRefresh(client, 0.1);
		return;
	}

	SchedulePendingInterruptMenu(client, 1.0);
}

void ResetPendingInterruptState(int client)
{
	bool hadDisplayedMenu = gB_PendingInterruptMenuDisplayed[client];

	gB_HasPendingInterrupt[client] = false;
	gB_PendingInterruptMenuDisplayed[client] = false;
	gF_PendingInterruptTime[client] = 0.0;
	gS_PendingInterruptMap[client][0] = '\0';
	gS_PendingInterruptRejectReason[client][0] = '\0';
	gB_PendingInterruptMapMatches[client] = false;
	gB_PendingInterruptLookupInFlight[client] = false;
	gB_PendingInterruptRestoreFetchInFlight[client] = false;
	gI_PendingInterruptRestoreState[client] = InterruptRestoreState_None;

	if (gH_PendingInterruptMenuTimer[client] != null)
	{
		KillTimer(gH_PendingInterruptMenuTimer[client]);
		gH_PendingInterruptMenuTimer[client] = null;
	}

	if (gH_PendingInterruptRefreshTimer[client] != null)
	{
		KillTimer(gH_PendingInterruptRefreshTimer[client]);
		gH_PendingInterruptRefreshTimer[client] = null;
	}

	if (hadDisplayedMenu && client > 0 && client <= MaxClients && IsClientInGame(client) && GetClientTeam(client) > CS_TEAM_SPECTATOR)
	{
		CancelClientMenu(client, true);
		GOKZ_HUD_ForceUpdateTPMenu(client);
	}
}

void ResetRunMonitorState(int client)
{
	gB_AutoInterruptSaveOnDisconnect[client] = false;
	gB_HasSafeSnapshot[client] = false;
	gF_NextPeriodicSnapshotAt[client] = 0.0;
	gI_LastObservedCheckpointCount[client] = 0;
	CleanupSnapshot(gSafeSnapshots[client]);
	InitializeSnapshot(gSafeSnapshots[client]);
}

Handle CreateInterruptPauseRequest(int client, const char[] endpoint)
{
	if (client <= 0 || client > MaxClients)
	{
		return null;
	}

	char apiBase[INTERRUPT_API_URL_MAX];
	char apiToken[INTERRUPT_API_TOKEN_MAX];
	char url[INTERRUPT_API_URL_MAX];
	char serverId[16];
	char authPrimary[MAX_AUTHID_LENGTH];
	char authSteamId64[MAX_AUTHID_LENGTH];
	char authSteam3[MAX_AUTHID_LENGTH];
	char authSteam2[MAX_AUTHID_LENGTH];
	char authEngine[MAX_AUTHID_LENGTH];

	gCV_InterruptPauseApiBaseUrl.GetString(apiBase, sizeof(apiBase));
	gCV_InterruptPauseApiToken.GetString(apiToken, sizeof(apiToken));
	TrimString(apiBase);
	TrimString(apiToken);

	if (apiBase[0] == '\0' || apiToken[0] == '\0')
	{
		DebugLog("interrupt api config missing for client=%N endpoint=%s", client, endpoint);
		return null;
	}

	if (!ResolvePrimaryAuth(client, authPrimary, sizeof(authPrimary)))
	{
		DebugLog("interrupt api primary auth missing for client=%N endpoint=%s", client, endpoint);
		return null;
	}

	authSteamId64[0] = '\0';
	authSteam3[0] = '\0';
	authSteam2[0] = '\0';
	authEngine[0] = '\0';
	GetClientAuthId(client, AuthId_SteamID64, authSteamId64, sizeof(authSteamId64));
	GetClientAuthId(client, AuthId_Steam3, authSteam3, sizeof(authSteam3));
	GetClientAuthId(client, AuthId_Steam2, authSteam2, sizeof(authSteam2));
	GetClientAuthId(client, AuthId_Engine, authEngine, sizeof(authEngine));

	if (apiBase[strlen(apiBase) - 1] == '/')
	{
		Format(url, sizeof(url), "%s%s", apiBase, endpoint);
	}
	else
	{
		Format(url, sizeof(url), "%s/%s", apiBase, endpoint);
	}

	Handle request = SteamWorks_CreateHTTPRequest(k_EHTTPMethodPOST, url);
	if (request == null)
	{
		DebugLog("interrupt api request create failed for client=%N endpoint=%s", client, endpoint);
		return null;
	}

	IntToString(gCV_InterruptPauseServerId.IntValue, serverId, sizeof(serverId));
	SteamWorks_SetHTTPRequestNetworkActivityTimeout(request, gCV_InterruptPauseApiTimeout.IntValue);
	SteamWorks_SetHTTPRequestHeaderValue(request, "X-Plugin-Token", apiToken);
	SteamWorks_SetHTTPRequestHeaderValue(request, "Content-Type", "application/x-www-form-urlencoded");
	SteamWorks_SetHTTPRequestGetOrPostParameter(request, "server_id", serverId);
	SteamWorks_SetHTTPRequestGetOrPostParameter(request, "auth_primary", authPrimary);
	SteamWorks_SetHTTPRequestGetOrPostParameter(request, "auth_steamid64", authSteamId64);
	SteamWorks_SetHTTPRequestGetOrPostParameter(request, "auth_steam3", authSteam3);
	SteamWorks_SetHTTPRequestGetOrPostParameter(request, "auth_steam2", authSteam2);
	SteamWorks_SetHTTPRequestGetOrPostParameter(request, "auth_engine", authEngine);
	return request;
}

bool SendInterruptPauseRequest(Handle request, SteamWorksHTTPRequestCompleted callback, int client, any contextData2 = 0)
{
	if (request == null)
	{
		return false;
	}

	SteamWorks_SetHTTPRequestContextValue(request, GetClientUserId(client), contextData2);
	SteamWorks_SetHTTPCallbacks(request, callback, INVALID_FUNCTION, INVALID_FUNCTION, GetMyHandle());
	if (!SteamWorks_SendHTTPRequest(request))
	{
		CloseHandle(request);
		return false;
	}

	return true;
}

void ReadInterruptPauseResponseBody(Handle request, char[] body, int bodyLen)
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

void ExtractInterruptPauseResponseValue(const char[] body, const char[] key, char[] output, int outputLen)
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

InterruptRestoreState ParseInterruptRestoreState(const char[] status)
{
	if (StrEqual(status, "pending"))
	{
		return InterruptRestoreState_Pending;
	}
	if (StrEqual(status, "approved"))
	{
		return InterruptRestoreState_Approved;
	}
	if (StrEqual(status, "rejected"))
	{
		return InterruptRestoreState_Rejected;
	}
	return InterruptRestoreState_None;
}

bool RequestPendingInterruptStateRefresh(int client)
{
	if (client <= 0 || client > MaxClients || !IsClientInGame(client) || IsFakeClient(client) || !gB_CanRestoreThisConnection[client] || gB_PendingInterruptLookupInFlight[client])
	{
		return false;
	}

	Handle request = CreateInterruptPauseRequest(client, "peek");
	if (request == null)
	{
		return false;
	}

	gB_PendingInterruptLookupInFlight[client] = true;
	if (!SendInterruptPauseRequest(request, OnPeekInterruptPauseSnapshotCompleted, client))
	{
		gB_PendingInterruptLookupInFlight[client] = false;
		return false;
	}

	return true;
}

public Action Timer_RunMonitor(Handle timer, any data)
{
	if (gH_RunMonitorTimer == null)
	{
		gH_RunMonitorTimer = timer;
	}

	for (int client = 1; client <= MaxClients; client++)
	{
		RefreshRunMonitorState(client);
	}

	return Plugin_Continue;
}

void RefreshRunMonitorState(int client)
{
	if (!ShouldMonitorClientRun(client))
	{
		if (gB_AutoInterruptSaveOnDisconnect[client] || gB_HasSafeSnapshot[client])
		{
			ResetRunMonitorState(client);
		}
		return;
	}

	float currentTime = GOKZ_GetTime(client);
	int checkpointCount = GOKZ_GetCheckpointCount(client);
	if (!gB_AutoInterruptSaveOnDisconnect[client])
	{
		ActivateRunMonitorState(client);
		DebugLog("monitor activated client=%N time=%.3f cps=%d", client, currentTime, checkpointCount);
	}

	bool checkpointChanged = checkpointCount != gI_LastObservedCheckpointCount[client];
	gI_LastObservedCheckpointCount[client] = checkpointCount;
	bool periodicSnapshotDue = GetGameTime() >= gF_NextPeriodicSnapshotAt[client];
	bool wroteThisTick = false;
	InterruptSnapshot currentSnapshot;
	bool hasCurrentSnapshot = CaptureSnapshot(client, currentSnapshot);
	bool currentSnapshotSafe = false;

	if (hasCurrentSnapshot)
	{
		currentSnapshotSafe = IsSnapshotSafeForSave(currentSnapshot);
		if (currentSnapshotSafe && StoreSafeSnapshot(client, currentSnapshot))
		{
			DebugLog("safe snapshot refreshed client=%N time=%.3f cps=%d", client, currentSnapshot.time, currentSnapshot.checkpointCount);
		}
	}
	else
	{
		CleanupSnapshot(currentSnapshot);
	}

	if (checkpointChanged)
	{
		RequestAsyncSnapshotWriteWithCurrentSnapshot(client, false, currentSnapshot, hasCurrentSnapshot, currentSnapshotSafe);
		wroteThisTick = true;
		DebugLog("monitor checkpoint-triggered save client=%N time=%.3f cps=%d", client, currentTime, checkpointCount);
	}

	if (periodicSnapshotDue)
	{
		if (!wroteThisTick)
		{
			RequestAsyncSnapshotWriteWithCurrentSnapshot(client, false, currentSnapshot, hasCurrentSnapshot, currentSnapshotSafe);
		}
		gF_NextPeriodicSnapshotAt[client] = GetGameTime() + PERIODIC_SNAPSHOT_INTERVAL_SECONDS;
		DebugLog("monitor periodic save client=%N time=%.3f cps=%d", client, currentTime, checkpointCount);
	}

	if (hasCurrentSnapshot)
	{
		CleanupSnapshot(currentSnapshot);
	}
}

void TryAutoSaveSnapshotOnDisconnect(int client)
{
	if (client <= 0 || client > MaxClients || !IsClientInGame(client) || IsFakeClient(client))
	{
		return;
	}

	if (!gB_AutoInterruptSaveOnDisconnect[client])
	{
		if (!ShouldMonitorClientRun(client))
		{
			return;
		}

		ActivateRunMonitorState(client);
	}

	if (!IsClientEligibleForInterruptSave(client))
	{
		return;
	}

	InterruptSnapshot snapshot;
	if (!BuildSnapshotForSave(client, true, snapshot))
	{
		CleanupSnapshot(snapshot);
		return;
	}

	WriteSnapshot(client, snapshot, InterruptSaveRequestKind_Auto);
	CleanupSnapshot(snapshot);
}

void ActivateRunMonitorState(int client)
{
	if (client <= 0 || client > MaxClients)
	{
		return;
	}

	gB_AutoInterruptSaveOnDisconnect[client] = true;
	gI_LastObservedCheckpointCount[client] = GOKZ_GetCheckpointCount(client);
	gF_NextPeriodicSnapshotAt[client] = GetGameTime() + PERIODIC_SNAPSHOT_INTERVAL_SECONDS;
}

void SchedulePendingInterruptRefresh(int client, float delay)
{
	if (client <= 0 || client > MaxClients)
	{
		return;
	}

	if (gH_PendingInterruptRefreshTimer[client] != null)
	{
		KillTimer(gH_PendingInterruptRefreshTimer[client]);
	}

	gH_PendingInterruptRefreshTimer[client] = CreateTimer(delay, Timer_RefreshPendingInterruptState, GetClientUserId(client), TIMER_REPEAT | TIMER_FLAG_NO_MAPCHANGE);
}

public Action Timer_RefreshPendingInterruptState(Handle timer, any userid)
{
	int client = GetClientOfUserId(userid);
	if (client <= 0 || client > MaxClients)
	{
		return Plugin_Stop;
	}

	if (!IsClientInGame(client) || IsFakeClient(client))
	{
		if (gH_PendingInterruptRefreshTimer[client] == timer)
		{
			gH_PendingInterruptRefreshTimer[client] = null;
		}
		return Plugin_Stop;
	}

	if (!CanResolveAnyClientAuth(client))
	{
		return Plugin_Continue;
	}

	if (gH_PendingInterruptRefreshTimer[client] == timer)
	{
		gH_PendingInterruptRefreshTimer[client] = null;
	}

	RequestPendingInterruptStateRefresh(client);
	return Plugin_Stop;
}

void SchedulePendingInterruptMenu(int client, float delay)
{
	if (!gB_HasPendingInterrupt[client])
	{
		return;
	}

	if (gH_PendingInterruptMenuTimer[client] != null)
	{
		KillTimer(gH_PendingInterruptMenuTimer[client]);
	}

	gH_PendingInterruptMenuTimer[client] = CreateTimer(delay, Timer_ShowPendingInterruptMenu, GetClientUserId(client), TIMER_FLAG_NO_MAPCHANGE);
}

public Action Timer_ShowPendingInterruptMenu(Handle timer, any userid)
{
	int client = GetClientOfUserId(userid);
	if (client <= 0 || client > MaxClients)
	{
		return Plugin_Stop;
	}

	if (gH_PendingInterruptMenuTimer[client] == timer)
	{
		gH_PendingInterruptMenuTimer[client] = null;
	}

	if (!IsClientInGame(client) || !gB_HasPendingInterrupt[client])
	{
		return Plugin_Stop;
	}

	if (!CanDisplayPendingInterruptMenu(client))
	{
		SchedulePendingInterruptMenu(client, 0.5);
		return Plugin_Stop;
	}

	ShowPendingInterruptMenu(client);
	return Plugin_Stop;
}

void ShowPendingInterruptMenu(int client)
{
	if (!CanUsePlayerCommand(client) || !gB_HasPendingInterrupt[client])
	{
		return;
	}

	char timeText[64];
	char title[384];
	char requestLabel[64];
	int requestDraw = gB_PendingInterruptMapMatches[client] ? ITEMDRAW_DEFAULT : ITEMDRAW_DISABLED;
	FormatRunTime(gF_PendingInterruptTime[client], timeText, sizeof(timeText));

	if (gB_PendingInterruptMapMatches[client])
	{
		Format(title, sizeof(title), "中断计时\n \n时间: %s\n地图: %s\n ", timeText, gS_PendingInterruptMap[client]);
	}
	else
	{
		Format(title, sizeof(title), "中断计时\n \n时间: %s\n存档地图: %s\n当前地图不可恢复\n ", timeText, gS_PendingInterruptMap[client]);
	}

	switch (gI_PendingInterruptRestoreState[client])
	{
		case InterruptRestoreState_Pending:
		{
			StrCat(title, sizeof(title), "审核状态: 待审核\n");
			strcopy(requestLabel, sizeof(requestLabel), "恢复申请审核中");
			requestDraw = ITEMDRAW_DISABLED;
		}
		case InterruptRestoreState_Approved:
		{
			StrCat(title, sizeof(title), "审核状态: 已授权\n");
			strcopy(requestLabel, sizeof(requestLabel), "恢复中断");
		}
		case InterruptRestoreState_Rejected:
		{
			Format(requestLabel, sizeof(requestLabel), "重新申请恢复");
			StrCat(title, sizeof(title), "审核状态: 已拒绝\n");
			if (gS_PendingInterruptRejectReason[client][0] != '\0')
			{
				StrCat(title, sizeof(title), "理由: ");
				StrCat(title, sizeof(title), gS_PendingInterruptRejectReason[client]);
				StrCat(title, sizeof(title), "\n");
			}
		}
		default:
		{
			StrCat(title, sizeof(title), "审核状态: 未申请\n");
			strcopy(requestLabel, sizeof(requestLabel), "申请恢复");
		}
	}

	CancelClientMenu(client, true);
	Menu menu = new Menu(MenuHandler_PendingInterrupt);
	menu.OptionFlags = MENUFLAG_NO_SOUND;
	menu.ExitButton = false;
	menu.Pagination = MENU_NO_PAGINATION;
	menu.SetTitle(title);
	menu.AddItem("request_restore", requestLabel, requestDraw);
	menu.AddItem("abort", "终止中断");
	menu.Display(client, MENU_TIME_FOREVER);
	gB_PendingInterruptMenuDisplayed[client] = true;
}

public int MenuHandler_PendingInterrupt(Menu menu, MenuAction action, int param1, int param2)
{
	if (action == MenuAction_End)
	{
		delete menu;
	}
	else if (action == MenuAction_Select)
	{
		if (param1 > 0 && param1 <= MaxClients)
		{
			gB_PendingInterruptMenuDisplayed[param1] = false;
		}
		if (param1 > 0 && param1 <= MaxClients && IsClientInGame(param1))
		{
			GOKZ_HUD_ForceUpdateTPMenu(param1);
		}

		char info[32];
		menu.GetItem(param2, info, sizeof(info));

		if (StrEqual(info, "request_restore"))
		{
			HandlePendingInterruptRestore(param1);
		}
		else if (StrEqual(info, "abort"))
		{
			HandlePendingInterruptAbort(param1);
		}
	}
	else if (action == MenuAction_Cancel)
	{
		if (param1 > 0 && param1 <= MaxClients && gB_HasPendingInterrupt[param1])
		{
			gB_PendingInterruptMenuDisplayed[param1] = false;
			if (IsClientInGame(param1))
			{
				GOKZ_HUD_ForceUpdateTPMenu(param1);
			}
			SchedulePendingInterruptMenu(param1, 0.2);
		}
	}

	return 0;
}

void HandlePendingInterruptRestore(int client)
{
	if (!CanUsePlayerCommand(client))
	{
		return;
	}

	if (!gB_PendingInterruptMapMatches[client])
	{
		ReplyToCommand(client, "[InterruptPause] 当前地图与存档地图不一致，无法恢复。");
		return;
	}

	if (!CanBeginInterruptPauseRestore(client))
	{
		return;
	}

	if (gI_PendingInterruptRestoreState[client] == InterruptRestoreState_Approved)
	{
		RequestApprovedInterruptPausePayload(client);
	}
	else
	{
		RequestInterruptPauseRestoreApproval(client);
	}
}

void HandlePendingInterruptAbort(int client)
{
	if (!CanUsePlayerCommand(client))
	{
		return;
	}

	Handle request = CreateInterruptPauseRequest(client, "abort");
	if (request == null)
	{
		ReplyToCommand(client, "[InterruptPause] 终止中断失败，无法连接后端服务。");
		return;
	}

	if (!SendInterruptPauseRequest(request, OnAbortInterruptPauseCompleted, client))
	{
		ReplyToCommand(client, "[InterruptPause] 终止中断失败，发送请求失败。");
		return;
	}
}

bool CanBeginInterruptPauseRestore(int client)
{
	if (!gB_CanRestoreThisConnection[client])
	{
		ReplyToCommand(client, "[InterruptPause] 你必须重新进服后才能恢复中断。");
		return false;
	}

	if (!IsPlayerAlive(client))
	{
		ReplyToCommand(client, "[InterruptPause] 你必须先出生，才能恢复中断进度。");
		return false;
	}

	if (GOKZ_GetTimerRunning(client) || GOKZ_GetPaused(client))
	{
		ReplyToCommand(client, "[InterruptPause] 你当前仍有未清空的 GOKZ 计时/暂停状态。请先用 !stop 或重新出生后再恢复。");
		return false;
	}

	return true;
}

void RequestInterruptPauseRestoreApproval(int client)
{
	Handle request = CreateInterruptPauseRequest(client, "request-restore");
	if (request == null)
	{
		ReplyToCommand(client, "[InterruptPause] 提交恢复申请失败，无法连接后端服务。");
		return;
	}

	if (!SendInterruptPauseRequest(request, OnRequestInterruptPauseRestoreCompleted, client))
	{
		ReplyToCommand(client, "[InterruptPause] 提交恢复申请失败，请稍后再试。");
		return;
	}
}

void RequestApprovedInterruptPausePayload(int client)
{
	if (gB_PendingInterruptRestoreFetchInFlight[client])
	{
		return;
	}

	Handle request = CreateInterruptPauseRequest(client, "fetch-approved");
	if (request == null)
	{
		ReplyToCommand(client, "[InterruptPause] 读取已授权存档失败，无法连接后端服务。");
		return;
	}

	gB_PendingInterruptRestoreFetchInFlight[client] = true;
	if (!SendInterruptPauseRequest(request, OnFetchApprovedInterruptPauseCompleted, client))
	{
		gB_PendingInterruptRestoreFetchInFlight[client] = false;
		ReplyToCommand(client, "[InterruptPause] 读取已授权存档失败，请稍后再试。");
		return;
	}
}

bool ApplyInterruptPausePayload(int client, const char[] payload)
{
	char auth[MAX_AUTHID_LENGTH];
	InterruptSnapshot snapshot;
	if (!ResolvePrimaryAuth(client, auth, sizeof(auth)) || !DeserializeSnapshotPayload(auth, payload, snapshot))
	{
		CleanupSnapshot(snapshot);
		ReplyToCommand(client, "[InterruptPause] 已授权存档内容无效，无法恢复。");
		return false;
	}

	if (!ValidateSnapshotIpForRestore(client, snapshot, true))
	{
		CleanupSnapshot(snapshot);
		ResetPendingInterruptState(client);
		return false;
	}

	char currentMap[PLATFORM_MAX_PATH];
	GetCurrentMap(currentMap, sizeof(currentMap));
	if (!StrEqual(currentMap, snapshot.map, false))
	{
		gB_PendingInterruptMapMatches[client] = false;
		DebugLog("restore map mismatch for client=%N stored=%s current=%s", client, snapshot.map, currentMap);
		CleanupSnapshot(snapshot);
		ReplyToCommand(client, "[InterruptPause] 存档地图是 %s，当前地图是 %s，不能恢复。", snapshot.map, currentMap);
		return false;
	}

	if (!ApplySnapshot(client, snapshot))
	{
		DebugLog("restore apply failed for client=%N auth=%s", client, snapshot.auth);
		CleanupSnapshot(snapshot);
		ReplyToCommand(client, "[InterruptPause] 恢复失败，GOKZ 没有接受该进度。");
		return false;
	}

	DebugLogGlobalState(client, "post-restore-after-apply");
	DebugLogGlobalState(client, "post-restore-before-cleanup");
	DebugLog("restore success for client=%N auth=%s map=%s time=%.3f", client, snapshot.auth, snapshot.map, snapshot.time);

	if (snapshot.penalizedForAirDisconnect)
	{
		ReplyToCommand(client, "[InterruptPause] 检测到异常断开。由于您断开时处于空中，已将您恢复至上一个存点。计时器已恢复。");
	}
	else if (snapshot.restoredFromSafePosition)
	{
		ReplyToCommand(client, "[InterruptPause] 检测到你断开时处于不安全状态，已将你恢复至上一个安全存点；计时当前保持暂停。");
	}
	else
	{
		ReplyToCommand(client, "[InterruptPause] 已恢复到上次中断位置，checkpoint/teleport/undo 与运动状态也已恢复，计时当前保持暂停。");
	}

	CleanupSnapshot(snapshot);
	return true;
}

public int OnPeekInterruptPauseSnapshotCompleted(Handle request, bool failure, bool requestSuccessful, EHTTPStatusCode statusCode, any contextData)
{
	int client = GetClientOfUserId(contextData);
	char body[INTERRUPT_RESPONSE_MAX];
	body[0] = '\0';
	ReadInterruptPauseResponseBody(request, body, sizeof(body));
	CloseHandle(request);

	if (client <= 0 || client > MaxClients)
	{
		return 0;
	}

	gB_PendingInterruptLookupInFlight[client] = false;

	if (!IsClientInGame(client) || IsFakeClient(client))
	{
		return 0;
	}

	if (failure || !requestSuccessful || view_as<int>(statusCode) >= 500)
	{
		DebugLog("peek interrupt snapshot failed for client=%N status=%d body=%s", client, view_as<int>(statusCode), body);
		return 0;
	}

	char status[32];
	char mapName[PLATFORM_MAX_PATH];
	char timeValue[32];
	char rejectReason[192];
	ExtractInterruptPauseResponseValue(body, "status=", status, sizeof(status));
	ExtractInterruptPauseResponseValue(body, "map_name=", mapName, sizeof(mapName));
	ExtractInterruptPauseResponseValue(body, "time_seconds=", timeValue, sizeof(timeValue));
	ExtractInterruptPauseResponseValue(body, "reject_reason=", rejectReason, sizeof(rejectReason));

	if (StrEqual(status, "none") || status[0] == '\0')
	{
		ResetPendingInterruptState(client);
		return 0;
	}

	gB_HasPendingInterrupt[client] = true;
	gF_PendingInterruptTime[client] = StringToFloat(timeValue);
	strcopy(gS_PendingInterruptMap[client], sizeof(gS_PendingInterruptMap[]), mapName);
	strcopy(gS_PendingInterruptRejectReason[client], sizeof(gS_PendingInterruptRejectReason[]), rejectReason);
	gI_PendingInterruptRestoreState[client] = ParseInterruptRestoreState(status);

	char currentMap[PLATFORM_MAX_PATH];
	GetCurrentMap(currentMap, sizeof(currentMap));
	gB_PendingInterruptMapMatches[client] = StrEqual(currentMap, mapName, false);

	if (gI_PendingInterruptRestoreState[client] == InterruptRestoreState_Pending)
	{
		SchedulePendingInterruptRefresh(client, INTERRUPT_REFRESH_INTERVAL_SECONDS);
	}

	if (CanDisplayPendingInterruptMenu(client))
	{
		SchedulePendingInterruptMenu(client, 0.5);
	}

	return 0;
}

public int OnWriteInterruptPauseSnapshotCompleted(Handle request, bool failure, bool requestSuccessful, EHTTPStatusCode statusCode, any contextData, any contextData2)
{
	int client = GetClientOfUserId(contextData);
	InterruptSaveRequestKind saveKind = view_as<InterruptSaveRequestKind>(contextData2);
	char body[INTERRUPT_RESPONSE_MAX];
	char message[256];
	body[0] = '\0';
	message[0] = '\0';
	ReadInterruptPauseResponseBody(request, body, sizeof(body));
	CloseHandle(request);
	ExtractInterruptPauseResponseValue(body, "message=", message, sizeof(message));

	if (saveKind == InterruptSaveRequestKind_Manual && client > 0 && client <= MaxClients)
	{
		gB_ManualInterruptSaveInFlight[client] = false;
	}

	if (!failure && requestSuccessful && view_as<int>(statusCode) < 400)
	{
		if (saveKind == InterruptSaveRequestKind_Manual
			&& client > 0
			&& client <= MaxClients
			&& IsClientInGame(client)
			&& !IsFakeClient(client))
		{
			gB_CanRestoreThisConnection[client] = false;
			ResetRunMonitorState(client);
			ResetPendingInterruptState(client);
			GOKZ_StopTimer(client, false);
			ReplyToCommand(client, "[InterruptPause] 已保存进度并中断本次计时；你现在可以继续自由移动。重新进服并回到相同地图后会弹出恢复菜单。");
		}
		return 0;
	}

	if (saveKind == InterruptSaveRequestKind_Manual
		&& client > 0
		&& client <= MaxClients
		&& IsClientInGame(client)
		&& !IsFakeClient(client))
	{
		if (message[0] == '\0')
		{
			strcopy(message, sizeof(message), "保存失败，无法写入后端中断存档。");
		}
		ReplyToCommand(client, "[InterruptPause] %s", message);
	}
	else if (client > 0 && client <= MaxClients && IsClientInGame(client))
	{
		DebugLog("interrupt snapshot save failed for client=%N status=%d body=%s", client, view_as<int>(statusCode), body);
	}
	else
	{
		LogError("[InterruptPause] snapshot save failed status=%d body=%s", view_as<int>(statusCode), body);
	}
	return 0;
}

public int OnRequestInterruptPauseRestoreCompleted(Handle request, bool failure, bool requestSuccessful, EHTTPStatusCode statusCode, any contextData)
{
	int client = GetClientOfUserId(contextData);
	char body[INTERRUPT_RESPONSE_MAX];
	body[0] = '\0';
	ReadInterruptPauseResponseBody(request, body, sizeof(body));
	CloseHandle(request);

	if (client <= 0 || client > MaxClients || !IsClientInGame(client) || IsFakeClient(client))
	{
		return 0;
	}

	if (failure || !requestSuccessful || view_as<int>(statusCode) >= 500)
	{
		ReplyToCommand(client, "[InterruptPause] 提交恢复申请失败，后端服务暂时不可用。");
		return 0;
	}

	char status[32];
	char message[256];
	char rejectReason[192];
	ExtractInterruptPauseResponseValue(body, "status=", status, sizeof(status));
	ExtractInterruptPauseResponseValue(body, "message=", message, sizeof(message));
	ExtractInterruptPauseResponseValue(body, "reject_reason=", rejectReason, sizeof(rejectReason));

	gI_PendingInterruptRestoreState[client] = ParseInterruptRestoreState(status);
	strcopy(gS_PendingInterruptRejectReason[client], sizeof(gS_PendingInterruptRejectReason[]), rejectReason);

	if (message[0] != '\0')
	{
		ReplyToCommand(client, "[InterruptPause] %s", message);
	}

	if (gI_PendingInterruptRestoreState[client] == InterruptRestoreState_Approved)
	{
		RequestApprovedInterruptPausePayload(client);
		return 0;
	}

	if (gI_PendingInterruptRestoreState[client] == InterruptRestoreState_Pending)
	{
		SchedulePendingInterruptRefresh(client, INTERRUPT_REFRESH_INTERVAL_SECONDS);
	}

	if (gB_HasPendingInterrupt[client])
	{
		SchedulePendingInterruptMenu(client, 0.5);
	}

	return 0;
}

public int OnFetchApprovedInterruptPauseCompleted(Handle request, bool failure, bool requestSuccessful, EHTTPStatusCode statusCode, any contextData)
{
	int client = GetClientOfUserId(contextData);
	char body[INTERRUPT_RESPONSE_MAX];
	body[0] = '\0';
	ReadInterruptPauseResponseBody(request, body, sizeof(body));
	CloseHandle(request);

	if (client <= 0 || client > MaxClients)
	{
		return 0;
	}

	gB_PendingInterruptRestoreFetchInFlight[client] = false;

	if (!IsClientInGame(client) || IsFakeClient(client))
	{
		return 0;
	}

	if (failure || !requestSuccessful || view_as<int>(statusCode) >= 500)
	{
		ReplyToCommand(client, "[InterruptPause] 读取已授权存档失败，后端服务暂时不可用。");
		return 0;
	}

	if (view_as<int>(statusCode) != 200)
	{
		char message[256];
		ExtractInterruptPauseResponseValue(body, "message=", message, sizeof(message));
		if (message[0] == '\0')
		{
			strcopy(message, sizeof(message), "恢复申请尚未通过审核。");
		}
		ReplyToCommand(client, "[InterruptPause] %s", message);
		RequestPendingInterruptStateRefresh(client);
		return 0;
	}

	if (!CanBeginInterruptPauseRestore(client))
	{
		return 0;
	}

	if (!ApplyInterruptPausePayload(client, body))
	{
		if (gB_HasPendingInterrupt[client])
		{
			SchedulePendingInterruptMenu(client, 0.5);
		}
		return 0;
	}

	RequestCompleteInterruptPauseRestore(client);
	ResetPendingInterruptState(client);
	return 0;
}

public int OnCompleteInterruptPauseRestoreCompleted(Handle request, bool failure, bool requestSuccessful, EHTTPStatusCode statusCode, any contextData)
{
	int client = GetClientOfUserId(contextData);
	char body[512];
	body[0] = '\0';
	ReadInterruptPauseResponseBody(request, body, sizeof(body));
	CloseHandle(request);

	if (client <= 0 || client > MaxClients || !IsClientInGame(client) || IsFakeClient(client))
	{
		return 0;
	}

	if (failure || !requestSuccessful || view_as<int>(statusCode) >= 400)
	{
		DebugLog("complete restore notify failed for client=%N status=%d body=%s", client, view_as<int>(statusCode), body);
	}

	return 0;
}

public int OnAbortInterruptPauseCompleted(Handle request, bool failure, bool requestSuccessful, EHTTPStatusCode statusCode, any contextData)
{
	int client = GetClientOfUserId(contextData);
	char body[512];
	body[0] = '\0';
	ReadInterruptPauseResponseBody(request, body, sizeof(body));
	CloseHandle(request);

	if (client <= 0 || client > MaxClients || !IsClientInGame(client) || IsFakeClient(client))
	{
		return 0;
	}

	if (failure || !requestSuccessful || view_as<int>(statusCode) >= 400)
	{
		ReplyToCommand(client, "[InterruptPause] 终止中断失败，请稍后再试。");
		return 0;
	}

	ResetPendingInterruptState(client);
	ReplyToCommand(client, "[InterruptPause] 已终止本次中断存档。");
	return 0;
}

public int OnAbortInterruptPauseSilentlyCompleted(Handle request, bool failure, bool requestSuccessful, EHTTPStatusCode statusCode, any contextData)
{
	char body[512];
	body[0] = '\0';
	ReadInterruptPauseResponseBody(request, body, sizeof(body));
	CloseHandle(request);
	return 0;
}

void RequestCompleteInterruptPauseRestore(int client)
{
	Handle request = CreateInterruptPauseRequest(client, "complete-restore");
	if (request == null)
	{
		return;
	}

	SendInterruptPauseRequest(request, OnCompleteInterruptPauseRestoreCompleted, client);
}

void RequestAbortInterruptPauseSnapshotSilently(int client)
{
	Handle request = CreateInterruptPauseRequest(client, "abort");
	if (request == null)
	{
		return;
	}

	SendInterruptPauseRequest(request, OnAbortInterruptPauseSilentlyCompleted, client);
}

void FormatRunTime(float time, char[] buffer, int maxlen)
{
	int totalSeconds = RoundToFloor(time);
	int minutes = totalSeconds / 60;
	int seconds = totalSeconds % 60;
	int milliseconds = RoundToFloor((time - float(totalSeconds)) * 1000.0);
	Format(buffer, maxlen, "%02d:%02d.%03d", minutes, seconds, milliseconds);
}


public void GOKZ_OnTimerStart_Post(int client, int course)
{
	ResetRunMonitorState(client);
	DebugLogGlobalState(client, "GOKZ_OnTimerStart_Post");
}

public void GOKZ_OnTimerEnd_Post(int client, int course, float time, int teleportsUsed)
{
	ResetRunMonitorState(client);
	DebugLogGlobalState(client, "GOKZ_OnTimerEnd_Post");
	DebugLog("global hook timer-end-post client=%N course=%d time=%.3f teleports=%d", client, course, time, teleportsUsed);
}

public void GOKZ_OnRunInvalidated(int client)
{
	ResetRunMonitorState(client);
	DebugLogGlobalState(client, "GOKZ_OnRunInvalidated");
	DebugLog("global hook run-invalidated client=%N", client);
}

public void GOKZ_OnTimerStopped(int client)
{
	ResetRunMonitorState(client);
	DebugLogGlobalState(client, "GOKZ_OnTimerStopped");
	DebugLog("global hook timer-stopped client=%N", client);
}

bool CanUsePlayerCommand(int client)
{
	if (client <= 0)
	{
		ReplyToCommand(client, "[InterruptPause] 该指令只能由游戏内玩家使用。");
		return false;
	}

	if (!IsClientInGame(client) || IsFakeClient(client))
	{
		return false;
	}

	return true;
}

bool IsClientEligibleForInterruptSave(int client)
{
	return client > 0
		&& client <= MaxClients
		&& IsClientInGame(client)
		&& !IsFakeClient(client)
		&& IsPlayerAlive(client)
		&& GOKZ_GetTimerRunning(client)
		&& GOKZ_GetValidTimer(client)
		&& !GOKZ_GetPaused(client)
		&& GOKZ_GetTime(client) > 0.0;
}

bool GetInterruptSaveEligibilityFailure(int client, char[] reason, int maxlen)
{
	reason[0] = '\0';

	if (!IsPlayerAlive(client))
	{
		strcopy(reason, maxlen, "你必须处于存活状态，且正在进行有效且未暂停的 GOKZ 计时，时间还必须大于 0，才能保存进度。");
		return false;
	}

	if (!GOKZ_GetTimerRunning(client))
	{
		strcopy(reason, maxlen, "你当前没有正在运行的 GOKZ 计时，无法保存。");
		return false;
	}

	if (!GOKZ_GetValidTimer(client))
	{
		strcopy(reason, maxlen, "你当前的 GOKZ 计时无效，无法保存中断进度。");
		return false;
	}

	if (GOKZ_GetPaused(client))
	{
		strcopy(reason, maxlen, "暂停中的 GOKZ 计时不能保存为中断进度；请先恢复计时。");
		return false;
	}

	if (GOKZ_GetTime(client) <= 0.0)
	{
		strcopy(reason, maxlen, "只有当计时时间大于 0 时，才能保存中断进度。");
		return false;
	}

	return true;
}

bool ShouldMonitorClientRun(int client)
{
	return IsClientEligibleForInterruptSave(client);
}

bool BuildSnapshotForSave(int client, bool disconnectSave, InterruptSnapshot snapshot)
{
	InitializeSnapshot(snapshot);

	InterruptSnapshot currentSnapshot;
	if (!CaptureSnapshot(client, currentSnapshot))
	{
		CleanupSnapshot(currentSnapshot);
		return false;
	}

	bool currentSafe = IsSnapshotSafeForSave(currentSnapshot);
	if (currentSafe)
	{
		StoreSafeSnapshot(client, currentSnapshot);
	}

	bool wroteSnapshot = false;
	if (currentSafe)
	{
		wroteSnapshot = CloneSnapshot(currentSnapshot, snapshot);
	}
	else if (gB_HasSafeSnapshot[client] && gSafeSnapshots[client].exists)
	{
		wroteSnapshot = CloneSnapshot(gSafeSnapshots[client], snapshot);
		if (wroteSnapshot)
		{
			snapshot.restoredFromSafePosition = true;
			if (disconnectSave && IsSnapshotAirborne(currentSnapshot))
			{
				snapshot.time += AIRBORNE_DISCONNECT_PENALTY_SECONDS;
				snapshot.penalizedForAirDisconnect = true;
			}
		}
	}
	else
	{
		wroteSnapshot = CloneSnapshot(currentSnapshot, snapshot);
	}

	CleanupSnapshot(currentSnapshot);
	return wroteSnapshot;
}

bool BuildSnapshotForSaveFromCurrentSnapshot(int client, bool disconnectSave, const InterruptSnapshot currentSnapshot, bool hasCurrentSnapshot, bool currentSafe, InterruptSnapshot snapshot)
{
	InitializeSnapshot(snapshot);

	if (!hasCurrentSnapshot)
	{
		return BuildSnapshotForSave(client, disconnectSave, snapshot);
	}

	if (currentSafe)
	{
		return CloneSnapshot(currentSnapshot, snapshot);
	}

	if (gB_HasSafeSnapshot[client] && gSafeSnapshots[client].exists)
	{
		bool wroteSnapshot = CloneSnapshot(gSafeSnapshots[client], snapshot);
		if (wroteSnapshot)
		{
			snapshot.restoredFromSafePosition = true;
			if (disconnectSave && IsSnapshotAirborne(currentSnapshot))
			{
				snapshot.time += AIRBORNE_DISCONNECT_PENALTY_SECONDS;
				snapshot.penalizedForAirDisconnect = true;
			}
		}
		return wroteSnapshot;
	}

	return CloneSnapshot(currentSnapshot, snapshot);
}

void RequestAsyncSnapshotWriteWithCurrentSnapshot(int client, bool disconnectSave, const InterruptSnapshot currentSnapshot, bool hasCurrentSnapshot, bool currentSafe)
{
	InterruptSnapshot snapshot;
	if (!BuildSnapshotForSaveFromCurrentSnapshot(client, disconnectSave, currentSnapshot, hasCurrentSnapshot, currentSafe, snapshot))
	{
		CleanupSnapshot(snapshot);
		return;
	}

	WriteSnapshotAsync(client, snapshot, InterruptSaveRequestKind_Auto);
	CleanupSnapshot(snapshot);
}

bool WriteSnapshotAsync(int client, const InterruptSnapshot snapshot, InterruptSaveRequestKind saveKind)
{
	char payload[SNAPSHOT_PAYLOAD_MAX];
	if (!SerializeSnapshotPayload(snapshot, payload, sizeof(payload)))
	{
		DebugLog("interrupt async write failed: serialize payload auth=%s", snapshot.auth);
		return false;
	}

	Handle request = CreateInterruptPauseRequest(client, "save");
	if (request == null)
	{
		DebugLog("interrupt async write failed: request create failed auth=%s", snapshot.auth);
		return false;
	}

	char playerName[128];
	char ip[SNAPSHOT_IP_MAX_LENGTH];
	char buffer[64];
	GetClientName(client, playerName, sizeof(playerName));
	GetClientIP(client, ip, sizeof(ip), true);

	FloatToString(snapshot.time, buffer, sizeof(buffer));
	SteamWorks_SetHTTPRequestGetOrPostParameter(request, "player_name", playerName);
	SteamWorks_SetHTTPRequestGetOrPostParameter(request, "ip_address", ip);
	SteamWorks_SetHTTPRequestGetOrPostParameter(request, "map_name", snapshot.map);
	IntToString(snapshot.mode, buffer, sizeof(buffer));
	SteamWorks_SetHTTPRequestGetOrPostParameter(request, "mode", buffer);
	IntToString(snapshot.course, buffer, sizeof(buffer));
	SteamWorks_SetHTTPRequestGetOrPostParameter(request, "course", buffer);
	FloatToString(snapshot.time, buffer, sizeof(buffer));
	SteamWorks_SetHTTPRequestGetOrPostParameter(request, "time_seconds", buffer);
	IntToString(snapshot.checkpointCount, buffer, sizeof(buffer));
	SteamWorks_SetHTTPRequestGetOrPostParameter(request, "checkpoint_count", buffer);
	IntToString(snapshot.teleportCount, buffer, sizeof(buffer));
	SteamWorks_SetHTTPRequestGetOrPostParameter(request, "teleport_count", buffer);
	IntToString(STORAGE_VERSION, buffer, sizeof(buffer));
	SteamWorks_SetHTTPRequestGetOrPostParameter(request, "storage_version", buffer);
	SteamWorks_SetHTTPRequestGetOrPostParameter(request, "payload", payload);

	if (saveKind == InterruptSaveRequestKind_Manual)
	{
		gB_ManualInterruptSaveInFlight[client] = true;
	}

	if (!SendInterruptPauseRequest(request, OnWriteInterruptPauseSnapshotCompleted, client, saveKind))
	{
		if (saveKind == InterruptSaveRequestKind_Manual)
		{
			gB_ManualInterruptSaveInFlight[client] = false;
		}
		DebugLog("interrupt async write failed: send request failed auth=%s", snapshot.auth);
		return false;
	}

	return true;
}

bool StoreSafeSnapshot(int client, const InterruptSnapshot snapshot)
{
	gB_HasSafeSnapshot[client] = false;
	CleanupSnapshot(gSafeSnapshots[client]);
	InitializeSnapshot(gSafeSnapshots[client]);

	if (!CloneSnapshot(snapshot, gSafeSnapshots[client]))
	{
		return false;
	}

	gB_HasSafeSnapshot[client] = true;
	return true;
}

bool CloneSnapshot(const InterruptSnapshot source, InterruptSnapshot snapshot)
{
	InitializeSnapshot(snapshot);
	strcopy(snapshot.auth, sizeof(snapshot.auth), source.auth);
	strcopy(snapshot.savedIp, sizeof(snapshot.savedIp), source.savedIp);
	strcopy(snapshot.map, sizeof(snapshot.map), source.map);
	snapshot.mode = source.mode;
	snapshot.course = source.course;
	snapshot.time = source.time;
	snapshot.origin[0] = source.origin[0];
	snapshot.origin[1] = source.origin[1];
	snapshot.origin[2] = source.origin[2];
	snapshot.angles[0] = source.angles[0];
	snapshot.angles[1] = source.angles[1];
	snapshot.angles[2] = source.angles[2];
	snapshot.groundEnt = source.groundEnt;
	snapshot.flags = source.flags;
	snapshot.velocity[0] = source.velocity[0];
	snapshot.velocity[1] = source.velocity[1];
	snapshot.velocity[2] = source.velocity[2];
	snapshot.duckAmount = source.duckAmount;
	snapshot.ducking = source.ducking;
	snapshot.ducked = source.ducked;
	snapshot.lastDuckTime = source.lastDuckTime;
	snapshot.duckSpeed = source.duckSpeed;
	snapshot.stamina = source.stamina;
	snapshot.movetype = source.movetype;
	snapshot.ladderNormal[0] = source.ladderNormal[0];
	snapshot.ladderNormal[1] = source.ladderNormal[1];
	snapshot.ladderNormal[2] = source.ladderNormal[2];
	snapshot.collisionGroup = source.collisionGroup;
	snapshot.waterJumpTime = source.waterJumpTime;
	snapshot.hasWalkMovedSinceLastJump = source.hasWalkMovedSinceLastJump;
	snapshot.ignoreLadderJumpTimeOffset = source.ignoreLadderJumpTimeOffset;
	snapshot.lastPositionAtFullCrouchSpeed[0] = source.lastPositionAtFullCrouchSpeed[0];
	snapshot.lastPositionAtFullCrouchSpeed[1] = source.lastPositionAtFullCrouchSpeed[1];
	snapshot.checkpointVersion = source.checkpointVersion;
	snapshot.checkpointCount = source.checkpointCount;
	snapshot.teleportCount = source.teleportCount;
	snapshot.hasUndoTeleportData = source.hasUndoTeleportData;
	snapshot.restoredFromSafePosition = source.restoredFromSafePosition;
	snapshot.penalizedForAirDisconnect = source.penalizedForAirDisconnect;
	snapshot.exists = source.exists;

	snapshot.checkpointData = CloneCheckpointData(source.checkpointData);
	if (source.checkpointData != null && snapshot.checkpointData == null)
	{
		CleanupSnapshot(snapshot);
		return false;
	}

	snapshot.undoTeleportData = CloneUndoTeleportData(source.undoTeleportData);
	if (source.undoTeleportData != null && snapshot.undoTeleportData == null)
	{
		CleanupSnapshot(snapshot);
		return false;
	}

	return true;
}

ArrayList CloneCheckpointData(ArrayList checkpointData)
{
	if (checkpointData == null)
	{
		return null;
	}

	ArrayList clone = new ArrayList(sizeof(Checkpoint));
	if (clone == null)
	{
		return null;
	}

	for (int i = 0; i < checkpointData.Length; i++)
	{
		Checkpoint checkpoint;
		checkpointData.GetArray(i, checkpoint, sizeof(Checkpoint));
		clone.PushArray(checkpoint, sizeof(Checkpoint));
	}

	return clone;
}

ArrayList CloneUndoTeleportData(ArrayList undoTeleportData)
{
	if (undoTeleportData == null)
	{
		return null;
	}

	ArrayList clone = new ArrayList(sizeof(UndoTeleportData));
	if (clone == null)
	{
		return null;
	}

	for (int i = 0; i < undoTeleportData.Length; i++)
	{
		UndoTeleportData undoData;
		undoTeleportData.GetArray(i, undoData, sizeof(UndoTeleportData));
		clone.PushArray(undoData, sizeof(UndoTeleportData));
	}

	return clone;
}

bool IsSnapshotSafeForSave(const InterruptSnapshot snapshot)
{
	return !IsSnapshotAirborne(snapshot) && !IsSnapshotInIllegalTrigger(snapshot);
}

bool IsSnapshotAirborne(const InterruptSnapshot snapshot)
{
	return snapshot.movetype == MOVETYPE_WALK && (snapshot.flags & FL_ONGROUND) == 0;
}

bool IsSnapshotInIllegalTrigger(const InterruptSnapshot snapshot)
{
	if (!snapshot.hasUndoTeleportData || snapshot.undoTeleportData == null || snapshot.undoTeleportData.Length <= 0)
	{
		return false;
	}

	UndoTeleportData undoData;
	snapshot.undoTeleportData.GetArray(0, undoData, sizeof(UndoTeleportData));
	return undoData.lastTeleportInBhopTrigger || undoData.lastTeleportInAntiCpTrigger;
}


bool CaptureSnapshot(int client, InterruptSnapshot snapshot)
{
	InitializeSnapshot(snapshot);

	if (!ResolvePrimaryAuth(client, snapshot.auth, sizeof(snapshot.auth)))
	{
		DebugLog("capture could not resolve auth for client=%N", client);
		return false;
	}
	if (!ResolveClientIp(client, snapshot.savedIp, sizeof(snapshot.savedIp)))
	{
		DebugLog("capture could not resolve ip for client=%N", client);
		return false;
	}

	GetCurrentMap(snapshot.map, sizeof(snapshot.map));
	snapshot.mode = GOKZ_GetCoreOption(client, Option_Mode);
	snapshot.course = GOKZ_GetCourse(client);
	snapshot.time = GOKZ_GetTime(client);
	GetClientAbsOrigin(client, snapshot.origin);
	GetClientEyeAngles(client, snapshot.angles);
	snapshot.groundEnt = GetEntPropEnt(client, Prop_Data, "m_hGroundEntity");
	snapshot.flags = GetEntityFlags(client);
	GetEntPropVector(client, Prop_Data, "m_vecVelocity", snapshot.velocity);
	snapshot.duckAmount = GetEntPropFloat(client, Prop_Send, "m_flDuckAmount");
	snapshot.ducking = GetEntProp(client, Prop_Send, "m_bDucking") != 0;
	snapshot.ducked = GetEntProp(client, Prop_Send, "m_bDucked") != 0;
	snapshot.lastDuckTime = GetEntPropFloat(client, Prop_Send, "m_flLastDuckTime");
	snapshot.duckSpeed = Movement_GetDuckSpeed(client);
	snapshot.stamina = GetEntPropFloat(client, Prop_Send, "m_flStamina");
	snapshot.movetype = Movement_GetMovetype(client);
	GetEntPropVector(client, Prop_Send, "m_vecLadderNormal", snapshot.ladderNormal);
	snapshot.collisionGroup = GetEntProp(client, Prop_Send, "m_CollisionGroup");
	snapshot.waterJumpTime = GetEntPropFloat(client, Prop_Data, "m_flWaterJumpTime");
	snapshot.hasWalkMovedSinceLastJump = GetEntProp(client, Prop_Data, "m_bHasWalkMovedSinceLastJump") != 0;
	snapshot.ignoreLadderJumpTimeOffset = GetEntPropFloat(client, Prop_Data, "m_ignoreLadderJumpTime") - GetGameTime();
	GetLastPositionAtFullCrouchSpeed(client, snapshot.lastPositionAtFullCrouchSpeed);
	snapshot.checkpointVersion = GOKZ_CHECKPOINT_VERSION;
	snapshot.checkpointCount = GOKZ_GetCheckpointCount(client);
	snapshot.teleportCount = GOKZ_GetTeleportCount(client);
	snapshot.checkpointData = GOKZ_GetCheckpointData(client);
	if (snapshot.checkpointData == null)
	{
		snapshot.checkpointData = new ArrayList(sizeof(Checkpoint));
	}
	if (snapshot.checkpointData == null)
	{
		DebugLog("apply failed: checkpointData null for client=%N", client);
		return false;
	}

	snapshot.undoTeleportData = GOKZ_GetUndoTeleportData(client);
	if (snapshot.undoTeleportData == null)
	{
		return false;
	}
	snapshot.hasUndoTeleportData = true;
	snapshot.restoredFromSafePosition = false;
	snapshot.penalizedForAirDisconnect = false;

	snapshot.exists = true;
	return true;
}

bool ApplySnapshot(int client, const InterruptSnapshot snapshot)
{
	if (snapshot.checkpointData == null)
	{
		return false;
	}

	if (!GOKZ_SetMode(client, snapshot.mode))
	{
		DebugLog("apply failed: GOKZ_SetMode client=%N mode=%d", client, snapshot.mode);
		return false;
	}

	GOKZ_SetCourse(client, snapshot.course);

	if (!GOKZ_SetTime(client, snapshot.time))
	{
		DebugLog("apply failed: GOKZ_SetTime client=%N time=%.3f", client, snapshot.time);
		return false;
	}

	if (!GOKZ_GetTimerRunning(client) || GOKZ_GetCourse(client) != snapshot.course)
	{
		DebugLog("apply failed: timer/course mismatch client=%N running=%d course=%d expected=%d", client, GOKZ_GetTimerRunning(client), GOKZ_GetCourse(client), snapshot.course);
		return false;
	}

	if (!GOKZ_SetCheckpointData(client, snapshot.checkpointData, snapshot.checkpointVersion))
	{
		DebugLog("apply failed: GOKZ_SetCheckpointData client=%N version=%d len=%d", client, snapshot.checkpointVersion, snapshot.checkpointData.Length);
		return false;
	}

	if (GOKZ_SetCheckpointCount(client, snapshot.checkpointCount) == 0)
	{
		DebugLog("apply failed: GOKZ_SetCheckpointCount client=%N count=%d", client, snapshot.checkpointCount);
		return false;
	}

	if (!GOKZ_SetTeleportCount(client, snapshot.teleportCount))
	{
		DebugLog("apply failed: GOKZ_SetTeleportCount client=%N count=%d", client, snapshot.teleportCount);
		return false;
	}

	if (snapshot.hasUndoTeleportData
		&& (snapshot.undoTeleportData == null
		|| !GOKZ_SetUndoTeleportData(client, snapshot.undoTeleportData, snapshot.checkpointVersion)))
	{
		DebugLog("apply failed: GOKZ_SetUndoTeleportData client=%N hasUndo=%d len=%d version=%d", client, snapshot.hasUndoTeleportData, snapshot.undoTeleportData == null ? -1 : snapshot.undoTeleportData.Length, snapshot.checkpointVersion);
		return false;
	}

	MoveType safeMovetype = GetSafeRestoreMovetype(snapshot);
	bool restoreGroundEntity = false;
	bool restoreLadderState = ShouldRestoreLadderState(snapshot, safeMovetype);
	float targetOrigin[3];
	float targetVelocity[3];
	targetOrigin[0] = snapshot.origin[0];
	targetOrigin[1] = snapshot.origin[1];
	targetOrigin[2] = snapshot.origin[2];
	targetVelocity[0] = snapshot.velocity[0];
	targetVelocity[1] = snapshot.velocity[1];
	targetVelocity[2] = snapshot.velocity[2];

	if (ShouldSnapAirborneRestoreToGround(snapshot, safeMovetype))
	{
		if (!TryResolveRecoverableOrigin(client, snapshot, targetOrigin))
		{
			DebugLog("apply failed: no recoverable origin client=%N", client);
			return false;
		}

		ZeroVector(targetVelocity);
	}

	int flagsMask = ~(FL_CLIENT | FL_FAKECLIENT | FL_GODMODE | FL_NOTARGET | FL_AIMTARGET);
	int safeFlags = (snapshot.flags & flagsMask) | (GetEntityFlags(client) & ~flagsMask);
	safeFlags &= ~FL_ONGROUND;
	if (restoreGroundEntity)
	{
		safeFlags |= FL_ONGROUND;
	}
	SetEntityFlags(client, safeFlags);

	if (restoreGroundEntity)
	{
		SetEntPropEnt(client, Prop_Data, "m_hGroundEntity", snapshot.groundEnt);
	}
	else
	{
		SetEntPropEnt(client, Prop_Data, "m_hGroundEntity", -1);
	}

	TeleportEntity(client, targetOrigin, snapshot.angles, targetVelocity);
	SetEntPropFloat(client, Prop_Send, "m_flDuckAmount", snapshot.duckAmount);
	SetEntProp(client, Prop_Send, "m_bDucking", snapshot.ducking ? 1 : 0);
	SetEntProp(client, Prop_Send, "m_bDucked", snapshot.ducked ? 1 : 0);
	SetEntPropFloat(client, Prop_Send, "m_flLastDuckTime", snapshot.lastDuckTime);
	Movement_SetDuckSpeed(client, snapshot.duckSpeed);
	SetEntPropFloat(client, Prop_Send, "m_flStamina", snapshot.stamina);
	Movement_SetMovetype(client, safeMovetype);
	if (restoreLadderState)
	{
		SetEntPropVector(client, Prop_Send, "m_vecLadderNormal", snapshot.ladderNormal);
	}
	else
	{
		float zeroLadderNormal[3] = {0.0, 0.0, 0.0};
		SetEntPropVector(client, Prop_Send, "m_vecLadderNormal", zeroLadderNormal);
	}
	SetEntProp(client, Prop_Send, "m_CollisionGroup", snapshot.collisionGroup);
	SetEntPropFloat(client, Prop_Data, "m_flWaterJumpTime", snapshot.waterJumpTime);
	SetEntProp(client, Prop_Data, "m_bHasWalkMovedSinceLastJump", snapshot.hasWalkMovedSinceLastJump ? 1 : 0);
	SetEntPropFloat(client, Prop_Data, "m_ignoreLadderJumpTime", snapshot.ignoreLadderJumpTimeOffset + GetGameTime());
	SetLastPositionAtFullCrouchSpeed(client, snapshot.lastPositionAtFullCrouchSpeed);

	if (!GOKZ_GetPaused(client))
	{
		GOKZ_Pause(client);
		if (!GOKZ_GetPaused(client))
		{
			DebugLog("apply failed: GOKZ_Pause did not stick for client=%N", client);
			return false;
		}
	}

	return true;
}

bool SerializeSnapshotPayload(const InterruptSnapshot snapshot, char[] payload, int maxlen)
{
	KeyValues kv = new KeyValues("snapshot");
	kv.SetString("map", snapshot.map);
	kv.SetString("saved_ip", snapshot.savedIp);
	kv.SetNum("mode", snapshot.mode);
	kv.SetNum("course", snapshot.course);
	kv.SetFloat("time", snapshot.time);
	kv.SetVector("origin", snapshot.origin);
	kv.SetVector("angles", snapshot.angles);
	kv.SetNum("ground_ent", snapshot.groundEnt);
	kv.SetNum("flags", snapshot.flags);
	kv.SetVector("velocity", snapshot.velocity);
	kv.SetFloat("duck_amount", snapshot.duckAmount);
	kv.SetNum("ducking", snapshot.ducking ? 1 : 0);
	kv.SetNum("ducked", snapshot.ducked ? 1 : 0);
	kv.SetFloat("last_duck_time", snapshot.lastDuckTime);
	kv.SetFloat("duck_speed", snapshot.duckSpeed);
	kv.SetFloat("stamina", snapshot.stamina);
	kv.SetNum("movetype", view_as<int>(snapshot.movetype));
	kv.SetVector("ladder_normal", snapshot.ladderNormal);
	kv.SetNum("collision_group", snapshot.collisionGroup);
	kv.SetFloat("water_jump_time", snapshot.waterJumpTime);
	kv.SetNum("has_walk_moved_since_last_jump", snapshot.hasWalkMovedSinceLastJump ? 1 : 0);
	kv.SetFloat("ignore_ladder_jump_time_offset", snapshot.ignoreLadderJumpTimeOffset);
	kv.SetFloat("full_crouch_pos_x", snapshot.lastPositionAtFullCrouchSpeed[0]);
	kv.SetFloat("full_crouch_pos_y", snapshot.lastPositionAtFullCrouchSpeed[1]);
	kv.SetNum("checkpoint_version", snapshot.checkpointVersion);
	kv.SetNum("checkpoint_count", snapshot.checkpointCount);
	kv.SetNum("teleport_count", snapshot.teleportCount);
	kv.SetNum("restored_from_safe_position", snapshot.restoredFromSafePosition ? 1 : 0);
	kv.SetNum("penalized_for_air_disconnect", snapshot.penalizedForAirDisconnect ? 1 : 0);
	kv.SetNum("storage_version", STORAGE_VERSION);

	bool success = WriteCheckpointDataToKv(kv, snapshot.checkpointData)
		&& WriteUndoTeleportDataToKv(kv, snapshot.undoTeleportData);
	if (success)
	{
		int written = kv.ExportToString(payload, maxlen);
		success = written > 0 && written < maxlen;
	}

	delete kv;
	return success;
}

bool DeserializeSnapshotPayload(const char[] auth, const char[] payload, InterruptSnapshot snapshot)
{
	InitializeSnapshot(snapshot);
	strcopy(snapshot.auth, sizeof(snapshot.auth), auth);

	KeyValues kv = new KeyValues("snapshot");
	if (!kv.ImportFromString(payload, "interruptpause-sqlite"))
	{
		delete kv;
		DebugLog("deserialize failed: invalid payload for auth=%s", auth);
		return false;
	}
	kv.Rewind();

	kv.GetString("map", snapshot.map, sizeof(snapshot.map));
	kv.GetString("saved_ip", snapshot.savedIp, sizeof(snapshot.savedIp));
	snapshot.mode = kv.GetNum("mode", -1);
	snapshot.course = kv.GetNum("course", 0);
	snapshot.time = kv.GetFloat("time", -1.0);
	kv.GetVector("origin", snapshot.origin);
	kv.GetVector("angles", snapshot.angles);
	snapshot.groundEnt = kv.GetNum("ground_ent", -1);
	snapshot.flags = kv.GetNum("flags", 0);
	kv.GetVector("velocity", snapshot.velocity);
	snapshot.duckAmount = kv.GetFloat("duck_amount", 0.0);
	snapshot.ducking = kv.GetNum("ducking", 0) != 0;
	snapshot.ducked = kv.GetNum("ducked", 0) != 0;
	snapshot.lastDuckTime = kv.GetFloat("last_duck_time", 0.0);
	snapshot.duckSpeed = kv.GetFloat("duck_speed", 0.0);
	snapshot.stamina = kv.GetFloat("stamina", 0.0);
	snapshot.movetype = view_as<MoveType>(kv.GetNum("movetype", view_as<int>(MOVETYPE_WALK)));
	kv.GetVector("ladder_normal", snapshot.ladderNormal);
	snapshot.collisionGroup = kv.GetNum("collision_group", 0);
	snapshot.waterJumpTime = kv.GetFloat("water_jump_time", 0.0);
	snapshot.hasWalkMovedSinceLastJump = kv.GetNum("has_walk_moved_since_last_jump", 0) != 0;
	snapshot.ignoreLadderJumpTimeOffset = kv.GetFloat("ignore_ladder_jump_time_offset", 0.0);
	snapshot.lastPositionAtFullCrouchSpeed[0] = kv.GetFloat("full_crouch_pos_x", 0.0);
	snapshot.lastPositionAtFullCrouchSpeed[1] = kv.GetFloat("full_crouch_pos_y", 0.0);
	snapshot.checkpointVersion = kv.GetNum("checkpoint_version", GOKZ_CHECKPOINT_VERSION);
	snapshot.checkpointCount = kv.GetNum("checkpoint_count", 0);
	snapshot.teleportCount = kv.GetNum("teleport_count", 0);
	snapshot.restoredFromSafePosition = kv.GetNum("restored_from_safe_position", 0) != 0;
	snapshot.penalizedForAirDisconnect = kv.GetNum("penalized_for_air_disconnect", 0) != 0;
	snapshot.checkpointData = ReadCheckpointDataFromKv(kv);
	snapshot.undoTeleportData = ReadUndoTeleportDataFromKv(kv);
	delete kv;

	if (snapshot.checkpointData == null)
	{
		return false;
	}
	if (snapshot.checkpointCount < 0)
	{
		snapshot.checkpointCount = 0;
	}
	if (snapshot.checkpointCount > snapshot.checkpointData.Length)
	{
		snapshot.checkpointCount = snapshot.checkpointData.Length;
	}
	if (snapshot.checkpointData.Length == 0)
	{
		snapshot.checkpointCount = 0;
	}
	if (snapshot.undoTeleportData != null && snapshot.undoTeleportData.Length == 1)
	{
		snapshot.hasUndoTeleportData = true;
	}
	else
	{
		if (snapshot.undoTeleportData != null)
		{
			delete snapshot.undoTeleportData;
		}
		snapshot.undoTeleportData = null;
		snapshot.hasUndoTeleportData = false;
	}

	snapshot.exists = snapshot.mode >= 0
		&& snapshot.time >= 0.0
		&& snapshot.checkpointVersion == GOKZ_CHECKPOINT_VERSION
		&& snapshot.checkpointData != null;
	if (!snapshot.exists)
	{
		CleanupSnapshot(snapshot);
	}

	return snapshot.exists;
}

bool WriteSnapshot(int client, const InterruptSnapshot snapshot, InterruptSaveRequestKind saveKind = InterruptSaveRequestKind_Auto)
{
	return WriteSnapshotAsync(client, snapshot, saveKind);
}

void DebugLog(const char[] fmt, any ...)
{
	if (gCV_InterruptPauseDebug == null || !gCV_InterruptPauseDebug.BoolValue)
	{
		return;
	}

	char buffer[256];
	VFormat(buffer, sizeof(buffer), fmt, 2);
	PrintToServer("[InterruptPause] %s", buffer);
	LogMessage("[InterruptPause] %s", buffer);
}

void DebugLogGlobalState(int client, const char[] stage)
{
	if (gCV_InterruptPauseDebug == null || !gCV_InterruptPauseDebug.BoolValue)
	{
		return;
	}

	if (client <= 0 || client > MaxClients || !IsClientInGame(client))
	{
		DebugLog("global-state stage=%s invalid-client=%d", stage, client);
		return;
	}

	char auth2[32];
	auth2[0] = '\0';
	GetClientAuthId(client, AuthId_Steam2, auth2, sizeof(auth2));

	DebugLog(
		"global-state stage=%s client=%N alive=%d paused=%d running=%d valid=%d mode=%d course=%d time=%.3f teleports=%d timeType=%d steam2=%s",
		stage,
		client,
		IsPlayerAlive(client),
		GOKZ_GetPaused(client),
		GOKZ_GetTimerRunning(client),
		GOKZ_GetValidTimer(client),
		GOKZ_GetCoreOption(client, Option_Mode),
		GOKZ_GetCourse(client),
		GOKZ_GetTime(client),
		GOKZ_GetTeleportCount(client),
		GOKZ_GetTimeTypeEx(GOKZ_GetTeleportCount(client)),
		auth2
	);
}

bool CanResolveAnyClientAuth(int client)
{
	char auth[MAX_AUTHID_LENGTH];
	return GetClientAuthId(client, AuthId_SteamID64, auth, sizeof(auth))
		|| GetClientAuthId(client, AuthId_Steam3, auth, sizeof(auth))
		|| GetClientAuthId(client, AuthId_Steam2, auth, sizeof(auth))
		|| GetClientAuthId(client, AuthId_Engine, auth, sizeof(auth));
}

bool ResolvePrimaryAuth(int client, char[] auth, int maxlen)
{
	return GetClientAuthId(client, AuthId_SteamID64, auth, maxlen)
		|| GetClientAuthId(client, AuthId_Steam3, auth, maxlen)
		|| GetClientAuthId(client, AuthId_Steam2, auth, maxlen)
		|| GetClientAuthId(client, AuthId_Engine, auth, maxlen);
}

bool ResolveClientIp(int client, char[] ip, int maxlen)
{
	ip[0] = '\0';
	return GetClientIP(client, ip, maxlen, true) && ip[0] != '\0';
}

bool ValidateSnapshotIpForRestore(int client, const InterruptSnapshot snapshot, bool notify)
{
	char currentIp[SNAPSHOT_IP_MAX_LENGTH];
	if (!ResolveClientIp(client, currentIp, sizeof(currentIp)))
	{
		DebugLog("restore ip validation failed: current ip unavailable client=%N auth=%s", client, snapshot.auth);
		if (notify)
		{
			ReplyToCommand(client, "[InterruptPause] 无法校验你当前的 IP，已拒绝恢复本次中断存档。");
		}
		RequestAbortInterruptPauseSnapshotSilently(client);
		return false;
	}

	if (snapshot.savedIp[0] != '\0' && StrEqual(snapshot.savedIp, currentIp, false))
	{
		return true;
	}

	if (snapshot.savedIp[0] == '\0')
	{
		DebugLog("restore ip validation failed: missing saved ip client=%N auth=%s currentIp=%s", client, snapshot.auth, currentIp);
		if (notify)
		{
			ReplyToCommand(client, "[InterruptPause] 该中断存档未记录保存时 IP。为防止代跳，已拒绝恢复并清除存档。");
		}
	}
	else
	{
		DebugLog("restore ip mismatch client=%N auth=%s savedIp=%s currentIp=%s", client, snapshot.auth, snapshot.savedIp, currentIp);
		if (notify)
		{
			ReplyToCommand(client, "[InterruptPause] 检测到你当前 IP 与保存时不同。为防止代跳，已拒绝恢复并清除该存档。");
		}
	}

	RequestAbortInterruptPauseSnapshotSilently(client);
	return false;
}

void InitializeSnapshot(InterruptSnapshot snapshot)
{
	snapshot.auth[0] = '\0';
	snapshot.savedIp[0] = '\0';
	snapshot.map[0] = '\0';
	snapshot.mode = -1;
	snapshot.course = 0;
	snapshot.time = -1.0;
	ZeroVector(snapshot.origin);
	ZeroVector(snapshot.angles);
	snapshot.groundEnt = -1;
	snapshot.flags = 0;
	ZeroVector(snapshot.velocity);
	snapshot.duckAmount = 0.0;
	snapshot.ducking = false;
	snapshot.ducked = false;
	snapshot.lastDuckTime = 0.0;
	snapshot.duckSpeed = 0.0;
	snapshot.stamina = 0.0;
	snapshot.movetype = MOVETYPE_WALK;
	ZeroVector(snapshot.ladderNormal);
	snapshot.collisionGroup = 0;
	snapshot.waterJumpTime = 0.0;
	snapshot.hasWalkMovedSinceLastJump = false;
	snapshot.ignoreLadderJumpTimeOffset = 0.0;
	snapshot.lastPositionAtFullCrouchSpeed[0] = 0.0;
	snapshot.lastPositionAtFullCrouchSpeed[1] = 0.0;
	snapshot.checkpointVersion = GOKZ_CHECKPOINT_VERSION;
	snapshot.checkpointCount = 0;
	snapshot.teleportCount = 0;
	snapshot.checkpointData = null;
	snapshot.undoTeleportData = null;
	snapshot.hasUndoTeleportData = false;
	snapshot.restoredFromSafePosition = false;
	snapshot.penalizedForAirDisconnect = false;
	snapshot.exists = false;
}

void CleanupSnapshot(InterruptSnapshot snapshot)
{
	if (snapshot.checkpointData != null)
	{
		delete snapshot.checkpointData;
		snapshot.checkpointData = null;
	}

	if (snapshot.undoTeleportData != null)
	{
		delete snapshot.undoTeleportData;
		snapshot.undoTeleportData = null;
	}

	snapshot.hasUndoTeleportData = false;
	snapshot.restoredFromSafePosition = false;
	snapshot.penalizedForAirDisconnect = false;
	snapshot.exists = false;
}

void ZeroVector(float vec[3])
{
	vec[0] = 0.0;
	vec[1] = 0.0;
	vec[2] = 0.0;
}

bool IsZeroVector3(const float vec[3])
{
	return FloatAbs(vec[0]) < 0.001 && FloatAbs(vec[1]) < 0.001 && FloatAbs(vec[2]) < 0.001;
}

MoveType GetSafeRestoreMovetype(const InterruptSnapshot snapshot)
{
	switch (snapshot.movetype)
	{
		case MOVETYPE_WALK, MOVETYPE_LADDER:
		{
			return snapshot.movetype;
		}
	}

	return MOVETYPE_WALK;
}

bool ShouldRestoreLadderState(const InterruptSnapshot snapshot, MoveType safeMovetype)
{
	return safeMovetype == MOVETYPE_LADDER && !IsZeroVector3(snapshot.ladderNormal);
}

bool CanDisplayPendingInterruptMenu(int client)
{
	return client > 0
		&& client <= MaxClients
		&& IsClientInGame(client)
		&& !IsFakeClient(client)
		&& gB_CanShowPendingInterruptMenu[client]
		&& GetClientTeam(client) > CS_TEAM_SPECTATOR
		&& IsPlayerAlive(client);
}

bool ShouldSnapAirborneRestoreToGround(const InterruptSnapshot snapshot, MoveType safeMovetype)
{
	return safeMovetype == MOVETYPE_WALK && (snapshot.flags & FL_ONGROUND) == 0;
}

bool TryResolveRecoverableOrigin(int client, const InterruptSnapshot snapshot, float origin[3])
{
	if (TryFindRecoverableGroundOrigin(client, snapshot, origin))
	{
		return true;
	}

	return TryGetStartPositionOrigin(client, origin);
}

bool TryFindRecoverableGroundOrigin(int client, const InterruptSnapshot snapshot, float origin[3])
{
	float mins[3], maxs[3], start[3], end[3];
	GetClientMins(client, mins);
	GetClientMaxs(client, maxs);

	start[0] = snapshot.origin[0];
	start[1] = snapshot.origin[1];
	start[2] = snapshot.origin[2] + 4.0;
	end[0] = snapshot.origin[0];
	end[1] = snapshot.origin[1];
	end[2] = snapshot.origin[2] - 4096.0;

	Handle trace = TR_TraceHullFilterEx(start, end, mins, maxs, MASK_PLAYERSOLID_BRUSHONLY, TraceFilter_IgnorePlayers, client);
	if (trace == null)
	{
		return false;
	}

	bool hit = TR_DidHit(trace);
	if (hit)
	{
		TR_GetEndPosition(origin, trace);
		origin[2] += 1.0;
	}

	delete trace;
	return hit;
}

bool TryGetStartPositionOrigin(int client, float origin[3])
{
	float angles[3];
	return GOKZ_GetStartPosition(client, origin, angles) != StartPositionType_Spawn
		|| !IsZeroVector3(origin);
}

public bool TraceFilter_IgnorePlayers(int entity, int contentsMask, any data)
{
	return entity > MaxClients || entity == 0;
}

void GetLastPositionAtFullCrouchSpeed(int client, float origin[2])
{
	int baseOffset = GetDuckSpeedBaseOffset();
	if (baseOffset < 0)
	{
		origin[0] = 0.0;
		origin[1] = 0.0;
		return;
	}
	origin[0] = GetEntDataFloat(client, baseOffset + 4);
	origin[1] = GetEntDataFloat(client, baseOffset + 8);
}

void SetLastPositionAtFullCrouchSpeed(int client, const float origin[2])
{
	int baseOffset = GetDuckSpeedBaseOffset();
	if (baseOffset < 0)
	{
		return;
	}
	SetEntDataFloat(client, baseOffset + 4, origin[0]);
	SetEntDataFloat(client, baseOffset + 8, origin[1]);
}

int GetDuckSpeedBaseOffset()
{
	if (!gB_DuckSpeedBaseOffsetCached)
	{
		gI_DuckSpeedBaseOffset = FindSendPropInfo("CBasePlayer", "m_flDuckSpeed");
		gB_DuckSpeedBaseOffsetCached = true;
	}

	return gI_DuckSpeedBaseOffset;
}

bool WriteCheckpointDataToKv(KeyValues kv, ArrayList checkpointData)
{
	if (checkpointData == null)
	{
		return false;
	}

	kv.DeleteKey("checkpoint_data");
	if (!kv.JumpToKey("checkpoint_data", true))
	{
		return false;
	}

	kv.SetNum("length", checkpointData.Length);
	for (int i = 0; i < checkpointData.Length; i++)
	{
		char indexKey[16];
		IntToString(i, indexKey, sizeof(indexKey));
		if (!kv.JumpToKey(indexKey, true))
		{
			kv.GoBack();
			return false;
		}

		Checkpoint checkpoint;
		checkpointData.GetArray(i, checkpoint, sizeof(Checkpoint));
		kv.SetVector("origin", checkpoint.origin);
		kv.SetVector("angles", checkpoint.angles);
		kv.SetVector("ladderNormal", checkpoint.ladderNormal);
		kv.SetNum("onLadder", checkpoint.onLadder ? 1 : 0);
		kv.SetNum("groundEnt", checkpoint.groundEnt);
		kv.GoBack();
	}

	kv.GoBack();
	return true;
}

bool WriteUndoTeleportDataToKv(KeyValues kv, ArrayList undoTeleportData)
{
	kv.DeleteKey("undo_teleport_data");

	if (undoTeleportData == null || undoTeleportData.Length != 1)
	{
		return true;
	}

	if (!kv.JumpToKey("undo_teleport_data", true))
	{
		return false;
	}

	UndoTeleportData undoData;
	undoTeleportData.GetArray(0, undoData, sizeof(UndoTeleportData));
	kv.SetVector("tempOrigin", undoData.tempOrigin);
	kv.SetVector("tempAngles", undoData.tempAngles);
	kv.SetVector("origin", undoData.origin);
	kv.SetVector("angles", undoData.angles);
	kv.SetNum("lastTeleportOnGround", undoData.lastTeleportOnGround ? 1 : 0);
	kv.SetNum("lastTeleportInBhopTrigger", undoData.lastTeleportInBhopTrigger ? 1 : 0);
	kv.SetNum("lastTeleportInAntiCpTrigger", undoData.lastTeleportInAntiCpTrigger ? 1 : 0);
	kv.GoBack();
	return true;
}

ArrayList ReadCheckpointDataFromKv(KeyValues kv)
{
	ArrayList checkpointData = new ArrayList(sizeof(Checkpoint));
	if (checkpointData == null)
	{
		return null;
	}

	if (!kv.JumpToKey("checkpoint_data", false))
	{
		return checkpointData;
	}

	int length = kv.GetNum("length", 0);
	for (int i = 0; i < length; i++)
	{
		char indexKey[16];
		IntToString(i, indexKey, sizeof(indexKey));
		if (!kv.JumpToKey(indexKey, false))
		{
			kv.GoBack();
			delete checkpointData;
			return null;
		}

		Checkpoint checkpoint;
		kv.GetVector("origin", checkpoint.origin);
		kv.GetVector("angles", checkpoint.angles);
		kv.GetVector("ladderNormal", checkpoint.ladderNormal);
		checkpoint.onLadder = kv.GetNum("onLadder", 0) != 0;
		checkpoint.groundEnt = kv.GetNum("groundEnt", -1);
		checkpointData.PushArray(checkpoint, sizeof(Checkpoint));
		kv.GoBack();
	}

	kv.GoBack();
	return checkpointData;
}

ArrayList ReadUndoTeleportDataFromKv(KeyValues kv)
{
	ArrayList undoTeleportData = new ArrayList(sizeof(UndoTeleportData));
	if (undoTeleportData == null)
	{
		return null;
	}

	if (kv.JumpToKey("undo_teleport_data", false))
	{
		UndoTeleportData undoData;
		kv.GetVector("tempOrigin", undoData.tempOrigin);
		kv.GetVector("tempAngles", undoData.tempAngles);
		kv.GetVector("origin", undoData.origin);
		kv.GetVector("angles", undoData.angles);
		undoData.lastTeleportOnGround = kv.GetNum("lastTeleportOnGround", 0) != 0;
		undoData.lastTeleportInBhopTrigger = kv.GetNum("lastTeleportInBhopTrigger", 0) != 0;
		undoData.lastTeleportInAntiCpTrigger = kv.GetNum("lastTeleportInAntiCpTrigger", 0) != 0;
		undoTeleportData.PushArray(undoData, sizeof(UndoTeleportData));
		kv.GoBack();
		return undoTeleportData;
	}

	delete undoTeleportData;
	return null;
}
