#include <sourcemod>

#include <sdktools>

#include <GlobalAPI>
#include <gokz/anticheat>
#include <gokz/core>
#include <gokz/global>
#include <gokz/replays>
#include <gokz/momsurffix>

#include <autoexecconfig>

#undef REQUIRE_EXTENSIONS
#undef REQUIRE_PLUGIN
#include <gokz/localdb>
#include <gokz/localranks>

#include <gokz/kzplayer>

#pragma newdecls required
#pragma semicolon 1



public Plugin myinfo = 
{
	name = "GOKZ Global", 
	author = "DanZay", 
	description = "Provides centralised records and bans via GlobalAPI", 
	version = GOKZ_VERSION, 
	url = GOKZ_SOURCE_URL
};

bool gB_GOKZLocalDB;

bool gB_APIKeyCheck;
bool gB_ModeCheck[MODE_COUNT];
bool gB_BannedCommandsCheck;
char gC_CurrentMap[64];
int gI_CurrentMapFileSize;
bool gB_InValidRun[MAXPLAYERS + 1];
bool gB_GloballyVerified[MAXPLAYERS + 1];
bool gB_EnforcerOnFreshMap;
bool gB_JustLateLoaded;
int gI_FPSMax[MAXPLAYERS + 1];
bool gB_waitingForFPSKick[MAXPLAYERS + 1];
bool gB_MapValidated;
int gI_MapID;
int gI_MapFileSize;
int gI_MapTier;

ConVar gCV_gokz_settings_enforcer;
ConVar gCV_gokz_warn_for_non_global_map;
ConVar gCV_gokz_global_debug;
ConVar gCV_EnforcedCVar[ENFORCEDCVAR_COUNT];

static void DebugGlobalLog(const char[] fmt, any ...)
{
	if (gCV_gokz_global_debug == null || !gCV_gokz_global_debug.BoolValue)
	{
		return;
	}

	char buffer[256];
	VFormat(buffer, sizeof(buffer), fmt, 2);
	PrintToServer("[gokz-global] %s", buffer);
	LogMessage("[gokz-global] %s", buffer);
}

static void DebugGlobalState(int client, const char[] stage, int course, float time, int teleportsUsed)
{
	if (gCV_gokz_global_debug == null || !gCV_gokz_global_debug.BoolValue)
	{
		return;
	}

	if (!IsValidClient(client))
	{
		DebugGlobalLog("state stage=%s invalid-client=%d", stage, client);
		return;
	}

	char steam2[32];
	char modeStr[32];
	GetClientAuthId(client, AuthId_Steam2, steam2, sizeof(steam2));
	KZPlayer player = KZPlayer(client);
	int mode = player.Mode;
	GOKZ_GL_GetModeString(mode, modeStr, sizeof(modeStr));
	bool mapCheck = MapCheck();
	bool globalsEnabled = gB_APIKeyCheck && gB_BannedCommandsCheck && gCV_gokz_settings_enforcer.BoolValue && gB_EnforcerOnFreshMap && mapCheck && gB_ModeCheck[mode];

	DebugGlobalLog(
		"state stage=%s client=%N steam2=%s alive=%d verified=%d invalidRun=%d running=%d valid=%d paused=%d mode=%d modeStr=%s course=%d time=%.3f teleports=%d globalsEnabled=%d apiKey=%d bannedCheck=%d settings=%d freshMap=%d mapValidated=%d mapId=%d mapFile=%d currentFile=%d modeCheck=%d",
		stage,
		client,
		steam2,
		IsPlayerAlive(client),
		gB_GloballyVerified[client],
		gB_InValidRun[client],
		GOKZ_GetTimerRunning(client),
		GOKZ_GetValidTimer(client),
		GOKZ_GetPaused(client),
		mode,
		modeStr,
		course,
		time,
		teleportsUsed,
		globalsEnabled,
		gB_APIKeyCheck,
		gB_BannedCommandsCheck,
		gCV_gokz_settings_enforcer.BoolValue,
		gB_EnforcerOnFreshMap,
		gB_MapValidated,
		gI_MapID,
		gI_MapFileSize,
		gI_CurrentMapFileSize,
		gB_ModeCheck[mode]
	);
	DebugGlobalLog(
		"state-breakdown stage=%s client=%N globalsEnabled=%d [apiKey=%d banned=%d settings=%d freshMap=%d mapCheck=%d modeCheck=%d] [mapValidated=%d mapId=%d mapFile=%d currentFile=%d]",
		stage,
		client,
		globalsEnabled,
		gB_APIKeyCheck,
		gB_BannedCommandsCheck,
		gCV_gokz_settings_enforcer.BoolValue,
		gB_EnforcerOnFreshMap,
		mapCheck,
		gB_ModeCheck[mode],
		gB_MapValidated,
		gI_MapID,
		gI_MapFileSize,
		gI_CurrentMapFileSize
	);
}

#include "gokz-global/api.sp"
#include "gokz-global/ban_player.sp"
#include "gokz-global/commands.sp"
#include "gokz-global/maptop_menu.sp"
#include "gokz-global/print_records.sp"
#include "gokz-global/send_run.sp"
#include "gokz-global/points.sp"



// =====[ PLUGIN EVENTS ]=====

public APLRes AskPluginLoad2(Handle myself, bool late, char[] error, int err_max)
{
	CreateNatives();
	RegPluginLibrary("gokz-global");
	gB_JustLateLoaded = late;
	return APLRes_Success;
}

public void OnPluginStart()
{
	if (FloatAbs(1.0 / GetTickInterval() - 128.0) > EPSILON)
	{
		SetFailState("gokz-global currently only supports 128 tickrate servers.");
	}
	if (FindCommandLineParam("-insecure") || FindCommandLineParam("-tools"))
	{
		SetFailState("gokz-global currently only supports VAC-secured servers.");
	}
	LoadTranslations("gokz-common.phrases");
	LoadTranslations("gokz-global.phrases");
	
	gB_APIKeyCheck = false;
	gB_MapValidated = false;
	gI_MapID = -1;
	gI_MapFileSize = -1;
	gI_MapTier = -1;
	
	for (int mode = 0; mode < MODE_COUNT; mode++)
	{
		gB_ModeCheck[mode] = false;
	}
	
	CreateConVars();
	CreateGlobalForwards();
	RegisterCommands();
}

public void OnAllPluginsLoaded()
{
	gB_GOKZLocalDB = LibraryExists("gokz-localdb");
	
	for (int client = 1; client <= MaxClients; client++)
	{
		if (IsClientInGame(client))
		{
			OnClientPutInServer(client);
		}
	}
}

public void OnLibraryAdded(const char[] name)
{
	gB_GOKZLocalDB = gB_GOKZLocalDB || StrEqual(name, "gokz-localdb");
}

public void OnLibraryRemoved(const char[] name)
{
	gB_GOKZLocalDB = gB_GOKZLocalDB && !StrEqual(name, "gokz-localdb");
}

Action IntegrityChecks(Handle timer)
{
	for (int client = 1; client <= MaxClients; client++)
	{
		if (IsValidClient(client) && !IsFakeClient(client))
		{
			QueryClientConVar(client, "fps_max", FPSCheck, client);
			QueryClientConVar(client, "m_yaw", MYAWCheck, client);
		}
	}

	for (int i = 0; i < BANNEDPLUGINCOMMAND_COUNT; i++)
	{
		if (CommandExists(gC_BannedPluginCommands[i]))
		{
			Handle bannedIterator = GetPluginIterator();
			char pluginName[128]; 
			bool foundPlugin = false;
			while (MorePlugins(bannedIterator))
			{
				Handle bannedPlugin = ReadPlugin(bannedIterator);
				GetPluginInfo(bannedPlugin, PlInfo_Name, pluginName, sizeof(pluginName));
				if (StrEqual(pluginName, gC_BannedPlugins[i]))
				{
					char pluginPath[128];
					GetPluginFilename(bannedPlugin, pluginPath, sizeof(pluginPath));
					ServerCommand("sm plugins unload %s", pluginPath);
					char disabledPath[256], enabledPath[256], pluginFile[4][128];
					int subfolders = ExplodeString(pluginPath, "/", pluginFile, sizeof(pluginFile), sizeof(pluginFile[]));
					BuildPath(Path_SM, disabledPath, sizeof(disabledPath), "plugins/disabled/%s", pluginFile[subfolders - 1]);
					BuildPath(Path_SM, enabledPath, sizeof(enabledPath), "plugins/%s", pluginPath);
					RenameFile(disabledPath, enabledPath);
					LogError("[KZ] %s cannot be loaded at the same time as gokz-global. %s has been disabled.", pluginName, pluginName);
					delete bannedPlugin;
					foundPlugin = true;
					break;
				}
				delete bannedPlugin;
			}
			if (!foundPlugin && gB_BannedCommandsCheck)
			{
				gB_BannedCommandsCheck = false;
				LogError("You can't have a plugin which implements the %s command. Please disable it and reload the map.", gC_BannedPluginCommands[i]);
			}
			delete bannedIterator;
		}
	}
	
	return Plugin_Handled;
}

public void FPSCheck(QueryCookie cookie, int client, ConVarQueryResult result, const char[] cvarName, const char[] cvarValue, any value)
{
	if (IsValidClient(client) && !IsFakeClient(client))
	{
		gI_FPSMax[client] = StringToInt(cvarValue);
		if (gI_FPSMax[client] > 0 && gI_FPSMax[client] < GL_FPS_MAX_MIN_VALUE)
		{
			if (!gB_waitingForFPSKick[client])
			{
				gB_waitingForFPSKick[client] = true;
				CreateTimer(GL_FPS_MAX_KICK_TIMEOUT, FPSKickPlayer, GetClientUserId(client), TIMER_FLAG_NO_MAPCHANGE);
				GOKZ_PrintToChat(client, true, "%t", "Warn Player fps_max");
				if (GOKZ_GetTimerRunning(client))
				{
					GOKZ_StopTimer(client, true);
				}
				else
				{
					GOKZ_EmitSoundToClient(client, GOKZ_SOUND_TIMER_STOP, _, "Timer Stop");
				}
			}
		}
		else
		{
			gB_waitingForFPSKick[client] = false;
		}
	}
}

public void MYAWCheck(QueryCookie cookie, int client, ConVarQueryResult result, const char[] cvarName, const char[] cvarValue, any value)
{
	if (IsValidClient(client) && !IsFakeClient(client) && StringToFloat(cvarValue) > GL_MYAW_MAX_VALUE)
	{
		KickClient(client, "%T", "Kick Player m_yaw", client);
	}
}

Action FPSKickPlayer(Handle timer, int userid)
{
	int client = GetClientOfUserId(userid);
	if (IsValidClient(client) && !IsFakeClient(client) && gB_waitingForFPSKick[client])
	{
		KickClient(client, "%T", "Kick Player fps_max", client);
	}
	
	return Plugin_Handled;
}



// =====[ CLIENT EVENTS ]=====

public void OnClientPutInServer(int client)
{
	gB_GloballyVerified[client] = false;
	gB_waitingForFPSKick[client] = false;
	OnClientPutInServer_PrintRecords(client);
}

// OnClientAuthorized is apparently too early
public void OnClientPostAdminCheck(int client)
{
	ResetPoints(client);
	
	if (GlobalAPI_IsInit() && !IsFakeClient(client))
	{
		CheckClientGlobalBan(client);
		UpdatePoints(client);
	}
}

public void GlobalAPI_OnInitialized()
{
	SetupAPI();
}


public Action GOKZ_OnTimerStart(int client, int course)
{
	KZPlayer player = KZPlayer(client);
	int mode = player.Mode;

	// We check the timer running to prevent spam when standing inside VB.
	if (gCV_gokz_warn_for_non_global_map.BoolValue
		&& GlobalAPI_HasAPIKey()
		&& !GlobalsEnabled(mode)
		&& !GOKZ_GetTimerRunning(client))
	{
		GOKZ_PrintToChat(client, true, "%t", "Warn Player Not Global Run");
	}

	return Plugin_Continue;
}

public void GOKZ_OnTimerStart_Post(int client, int course)
{
	KZPlayer player = KZPlayer(client);
	int mode = player.Mode;
	gB_InValidRun[client] = GlobalsEnabled(mode);
	DebugGlobalState(client, "timer-start-post", course, 0.0, GOKZ_GetTeleportCount(client));
}

public void GOKZ_OnTimerEnd_Post(int client, int course, float time, int teleportsUsed)
{
	DebugGlobalState(client, "timer-end-post", course, time, teleportsUsed);
	if (gB_GloballyVerified[client] && gB_InValidRun[client])
	{
		DebugGlobalLog("sendtime gate passed client=%N course=%d time=%.3f teleports=%d", client, course, time, teleportsUsed);
		SendTime(client, course, time, teleportsUsed);
	}
	else
	{
		DebugGlobalLog("sendtime gate blocked client=%N globallyVerified=%d invalidRun=%d", client, gB_GloballyVerified[client], gB_InValidRun[client]);
	}
}

public Action GOKZ_RP_OnReplaySaved(int client, int replayType, const char[] map, int course, int timeType, float time, const char[] filePath, bool tempReplay)
{
	if (gB_GloballyVerified[client] && gB_InValidRun[client])
	{
		OnReplaySaved_SendReplay(client, replayType, map, course, timeType, time, filePath, tempReplay);
		return Plugin_Handled;
	}
	return Plugin_Continue;
}

public void GOKZ_OnRunInvalidated(int client)
{
	gB_InValidRun[client] = false;
	DebugGlobalState(client, "run-invalidated", GOKZ_GetCourse(client), GOKZ_GetTime(client), GOKZ_GetTeleportCount(client));
}

public void GOKZ_GL_OnNewTopTime(int client, int course, int mode, int timeType, int rank, int rankOverall, float runTime)
{
	AnnounceNewTopTime(client, course, mode, timeType, rank, rankOverall);
}

public void GOKZ_AC_OnPlayerSuspected(int client, ACReason reason, const char[] notes, const char[] stats)
{
	if (!gB_GloballyVerified[client])
	{
		return;
	}
	
	GlobalBanPlayer(client, reason, notes, stats);
	gB_GloballyVerified[client] = false;
}



// =====[ OTHER EVENTS ]=====

public void OnMapStart()
{
	LoadSounds();

	GetCurrentMapDisplayName(gC_CurrentMap, sizeof(gC_CurrentMap));
	gI_CurrentMapFileSize = GetCurrentMapFileSize();
	
	gB_BannedCommandsCheck = true;
	
	// Prevent just reloading the plugin after messing with the map
	if (gB_JustLateLoaded)
	{
		gB_JustLateLoaded = false;
	}
	else
	{
		gB_EnforcerOnFreshMap = true;
	}
	
	// In case of late loading
	if (GlobalAPI_IsInit())
	{
		GlobalAPI_OnInitialized();
	}
	
	// Setup a timer to monitor server/client integrity
	CreateTimer(1.0, IntegrityChecks, INVALID_HANDLE, TIMER_FLAG_NO_MAPCHANGE | TIMER_REPEAT);
}

public void OnMapEnd()
{
	// So it doesn't get carried over to the next map
	gI_MapID = -1;
	for (int client = 1; client < MaxClients; client++)
	{
		ResetMapPoints(client);
	}
}

public void GOKZ_OnOptionChanged(int client, const char[] option, any newValue)
{
	if (StrEqual(option, gC_CoreOptionNames[Option_Mode])
		&& GlobalAPI_IsInit() && IsClientAuthorized(client))
	{
		UpdatePoints(client);
	}
}

public void GOKZ_OnModeUnloaded(int mode)
{
	gB_ModeCheck[mode] = false;
}

public Action GOKZ_OnTimerNativeCalledExternally(Handle plugin, int client)
{
	char pluginName[64];
	char pluginFile[PLATFORM_MAX_PATH];
	GetPluginInfo(plugin, PlInfo_Name, pluginName, sizeof(pluginName));
	GetPluginFilename(plugin, pluginFile, sizeof(pluginFile));
	if (StrEqual(pluginFile, "gokz-interruptpause.smx", false))
	{
		KZPlayer player = KZPlayer(client);
		int mode = player.Mode;
		gB_InValidRun[client] = GlobalsEnabled(mode);
		if (GOKZ_GetTimerRunning(client))
		{
			DebugGlobalLog("allowed external gokz-core timer native client=%N plugin=\"%s\" file=%s rebuiltInvalidRun=%d", client, pluginName, pluginFile, gB_InValidRun[client]);
		}
		DebugGlobalState(client, "external-native-whitelist", GOKZ_GetCourse(client), GOKZ_GetTime(client), GOKZ_GetTeleportCount(client));
		return Plugin_Continue;
	}

	if (GOKZ_GetTimerRunning(client) && GOKZ_GetValidTimer(client))
	{
		DebugGlobalLog("invalidated run due to external gokz-core timer native client=%N plugin=\"%s\" file=%s", client, pluginName, pluginFile);
	}
	GOKZ_InvalidateRun(client);
	return Plugin_Continue;
}



// =====[ PUBLIC ]=====

bool GlobalsEnabled(int mode)
{
	return gB_APIKeyCheck && gB_BannedCommandsCheck && gCV_gokz_settings_enforcer.BoolValue && gB_EnforcerOnFreshMap && MapCheck() && gB_ModeCheck[mode];
}

bool MapCheck()
{
	return gB_MapValidated
	 && gI_MapID > 0
	 && gI_MapFileSize == gI_CurrentMapFileSize;
}

void PrintGlobalCheckToChat(int client)
{
	GOKZ_PrintToChat(client, true, "%t", "Global Check Header");
	GOKZ_PrintToChat(client, false, "%t", "Global Check", 
		gB_APIKeyCheck ? "{green}✓" : "{darkred}X", 
		gB_BannedCommandsCheck ? "{green}✓" : "{darkred}X",
		gCV_gokz_settings_enforcer.BoolValue && gB_EnforcerOnFreshMap ? "{green}✓" : "{darkred}X", 
		MapCheck() ? "{green}✓" : "{darkred}X", 
		gB_GloballyVerified[client] ? "{green}✓" : "{darkred}X");
	
	char modeCheck[256];
	FormatEx(modeCheck, sizeof(modeCheck), "{purple}%s %s", gC_ModeNames[0], gB_ModeCheck[0] ? "{green}✓" : "{darkred}X");
	for (int i = 1; i < MODE_COUNT; i++)
	{
		FormatEx(modeCheck, sizeof(modeCheck), "%s {grey}| {purple}%s %s", modeCheck, gC_ModeNames[i], gB_ModeCheck[i] ? "{green}✓" : "{darkred}X");
	}
	GOKZ_PrintToChat(client, false, "%s", modeCheck);
}

void AnnounceNewTopTime(int client, int course, int mode, int timeType, int rank, int rankOverall)
{
	bool newRecord = false;
	
	if (timeType == TimeType_Nub && rankOverall != 0)
	{
		if (rankOverall == 1)
		{
			if (course == 0)
			{
				GOKZ_PrintToChatAll(true, "%t", "New Global Record (NUB)", client, gC_ModeNamesShort[mode]);
			}
			else
			{
				GOKZ_PrintToChatAll(true, "%t", "New Global Bonus Record (NUB)", client, course, gC_ModeNamesShort[mode]);
			}
			newRecord = true;
		}
		else
		{
			if (course == 0)
			{
				GOKZ_PrintToChatAll(true, "%t", "New Global Top Time (NUB)", client, rankOverall, gC_ModeNamesShort[mode]);
			}
			else
			{
				GOKZ_PrintToChatAll(true, "%t", "New Global Top Bonus Time (NUB)", client, rankOverall, course, gC_ModeNamesShort[mode]);
			}
		}
	}
	else if (timeType == TimeType_Pro)
	{
		if (rankOverall != 0)
		{
			if (rankOverall == 1)
			{
				if (course == 0)
				{
					GOKZ_PrintToChatAll(true, "%t", "New Global Record (NUB)", client, gC_ModeNamesShort[mode]);
				}
				else
				{
					GOKZ_PrintToChatAll(true, "%t", "New Global Bonus Record (NUB)", client, course, gC_ModeNamesShort[mode]);
				}
				newRecord = true;
			}
			else
			{
				if (course == 0)
				{
					GOKZ_PrintToChatAll(true, "%t", "New Global Top Time (NUB)", client, rankOverall, gC_ModeNamesShort[mode]);
				}
				else
				{
					GOKZ_PrintToChatAll(true, "%t", "New Global Top Bonus Time (NUB)", client, rankOverall, course, gC_ModeNamesShort[mode]);
				}
			}
		}
		
		if (rank == 1)
		{
			if (course == 0)
			{
				GOKZ_PrintToChatAll(true, "%t", "New Global Record (PRO)", client, gC_ModeNamesShort[mode]);
			}
			else
			{
				GOKZ_PrintToChatAll(true, "%t", "New Global Bonus Record (PRO)", client, course, gC_ModeNamesShort[mode]);
			}
			newRecord = true;
		}
		else
		{
			if (course == 0)
			{
				GOKZ_PrintToChatAll(true, "%t", "New Global Top Time (PRO)", client, rank, gC_ModeNamesShort[mode]);
			}
			else
			{
				GOKZ_PrintToChatAll(true, "%t", "New Global Top Bonus Time (PRO)", client, rank, course, gC_ModeNamesShort[mode]);
			}
		}
	}
	
	if (newRecord)
	{
		PlayBeatRecordSound();
	}
}

void PlayBeatRecordSound()
{
	GOKZ_EmitSoundToAll(GL_SOUND_NEW_RECORD, _, "World Record");
}



// =====[ PRIVATE ]=====

static void CreateConVars()
{
	AutoExecConfig_SetFile("gokz-global", "sourcemod/gokz");
	AutoExecConfig_SetCreateFile(true);
	
	gCV_gokz_settings_enforcer = AutoExecConfig_CreateConVar("gokz_settings_enforcer", "1", "Whether GOKZ enforces convars required for global records.", _, true, 0.0, true, 1.0);
	gCV_gokz_warn_for_non_global_map = AutoExecConfig_CreateConVar("gokz_warn_for_non_global_map", "1", "Whether or not GOKZ should warn players if the global check does not pass.", _, true, 0.0, true, 1.0);
	gCV_gokz_global_debug = AutoExecConfig_CreateConVar("gokz_global_debug", "0", "Whether gokz-global should print terminal debug logs for submission gating.", _, true, 0.0, true, 1.0);
	gCV_gokz_settings_enforcer.AddChangeHook(OnConVarChanged);
	
	AutoExecConfig_ExecuteFile();
	AutoExecConfig_CleanFile();
	
	for (int i = 0; i < ENFORCEDCVAR_COUNT; i++)
	{
		gCV_EnforcedCVar[i] = FindConVar(gC_EnforcedCVars[i]);
		gCV_EnforcedCVar[i].FloatValue = gF_EnforcedCVarValues[i];
		gCV_EnforcedCVar[i].AddChangeHook(OnEnforcedConVarChanged);
	}
}

public void OnConVarChanged(ConVar convar, const char[] oldValue, const char[] newValue)
{
	if (convar == gCV_gokz_settings_enforcer)
	{
		if (gCV_gokz_settings_enforcer.BoolValue)
		{
			for (int i = 0; i < ENFORCEDCVAR_COUNT; i++)
			{
				gCV_EnforcedCVar[i].FloatValue = gF_EnforcedCVarValues[i];
			}
		}
		else
		{
			for (int client = 1; client <= MaxClients; client++)
			{
				gB_InValidRun[client] = false;
			}
			
			// You have to change map before you can re-activate that
			gB_EnforcerOnFreshMap = false;
		}
	}
}

public void OnEnforcedConVarChanged(ConVar convar, const char[] oldValue, const char[] newValue)
{
	if (gCV_gokz_settings_enforcer.BoolValue)
	{
		for (int i = 0; i < ENFORCEDCVAR_COUNT; i++)
		{
			if (convar == gCV_EnforcedCVar[i])
			{
				gCV_EnforcedCVar[i].FloatValue = gF_EnforcedCVarValues[i];
				return;
			}
		}
	}
}

static void SetupAPI()
{
	GlobalAPI_GetAuthStatus(GetAuthStatusCallback);
	GlobalAPI_GetModes(GetModeInfoCallback);
	GlobalAPI_GetMapByName(GetMapCallback, _, gC_CurrentMap);
	
	for (int client = 1; client <= MaxClients; client++)
	{
		if (IsValidClient(client) && !IsFakeClient(client))
		{
			CheckClientGlobalBan(client);
		}
	}
}

public int GetAuthStatusCallback(JSON_Object auth_json, GlobalAPIRequestData request)
{
	if (request.Failure)
	{
		LogError("Failed to check API key with Global API.");
		return 0;
	}
	
	APIAuth auth = view_as<APIAuth>(auth_json);
	if (!auth.IsValid)
	{
		LogError("Global API key was found to be missing or invalid.");
	}
	gB_APIKeyCheck = auth.IsValid;
	return 0;
}

public int GetModeInfoCallback(JSON_Object modes, GlobalAPIRequestData request)
{
	if (request.Failure)
	{
		LogError("Failed to check mode versions with Global API.");
		return 0;
	}
	
	if (!modes.IsArray)
	{
		LogError("GlobalAPI returned a malformed response while looking up the modes.");
		return 0;
	}
	
	for (int i = 0; i < modes.Length; i++)
	{
		APIMode mode = view_as<APIMode>(modes.GetObjectIndexed(i));
		int mode_id = GOKZ_GL_FromGlobalMode(view_as<GlobalMode>(mode.Id));
		if (mode_id == -1)
		{
			LogError("GlobalAPI returned a malformed mode.");
		}
		else if (mode.LatestVersion <= GOKZ_GetModeVersion(mode_id))
		{
			gB_ModeCheck[mode_id] = true;
		}
		else
		{
			char desc[128];
			
			gB_ModeCheck[mode_id] = false;
			mode.GetLatestVersionDesc(desc, sizeof(desc));
			LogError("Global API requires %s mode version %d (%s). You have version %d (%s).", 
				gC_ModeNames[mode_id], mode.LatestVersion, desc, GOKZ_GetModeVersion(mode_id), GOKZ_VERSION);
		}
	}
	return 0;
}

public int GetMapCallback(JSON_Object map_json, GlobalAPIRequestData request)
{
	if (request.Failure || map_json == INVALID_HANDLE)
	{
		DebugGlobalLog("getmap-callback failure currentMap=%s", gC_CurrentMap);
		LogError("Failed to get map info.");
		return 0;
	}
	
	APIMap map = view_as<APIMap>(map_json);
	
	gB_MapValidated = map.IsValidated;
	gI_MapID = map.Id;
	gI_MapFileSize = map.Filesize;
	gI_MapTier = map.Difficulty;
	DebugGlobalLog("getmap-callback success map=%s mapId=%d validated=%d apiFile=%d currentFile=%d tier=%d", gC_CurrentMap, gI_MapID, gB_MapValidated, gI_MapFileSize, gI_CurrentMapFileSize, gI_MapTier);
	
	// We don't do that earlier cause we need the map ID
	for (int client = 1; client <= MaxClients; client++)
	{
		if (IsValidClient(client) && IsClientAuthorized(client) && !IsFakeClient(client))
		{
			UpdatePoints(client);
		}
	}
	return 0;
}

void CheckClientGlobalBan(int client)
{
	char steamid[32];
	GetClientAuthId(client, AuthId_Steam2, steamid, sizeof(steamid));
	GlobalAPI_GetPlayerBySteamId(CheckClientGlobalBan_Callback, client, steamid);
}

public void CheckClientGlobalBan_Callback(JSON_Object player_json, GlobalAPIRequestData request, int client)
{
	if (!IsValidClient(client))
	{
		return;
	}
	
	if (request.Failure)
	{
		DebugGlobalLog("globalban-callback failure client=%d", client);
		LogError("Failed to get ban info.");
		return;
	}
	
	char client_steamid[32], response_steamid[32];
	GetClientAuthId(client, AuthId_Steam2, client_steamid, sizeof(client_steamid));
	
	if (!player_json.IsArray || player_json.Length != 1)
	{
		DebugGlobalLog("globalban-callback malformed client=%N steam2=%s", client, client_steamid);
		LogError("Got malformed reply when querying steamid %s", client_steamid);
		return;
	}
	
	APIPlayer player = view_as<APIPlayer>(player_json.GetObjectIndexed(0));
	player.GetSteamId(response_steamid, sizeof(response_steamid));
	if (!StrEqual(client_steamid, response_steamid))
	{
		DebugGlobalLog("globalban-callback steamid mismatch client=%N requested=%s response=%s", client, client_steamid, response_steamid);
		return;
	}
	
	gB_GloballyVerified[client] = !player.IsBanned;
	DebugGlobalLog("globalban-callback success client=%N steam2=%s verified=%d banned=%d", client, client_steamid, gB_GloballyVerified[client], player.IsBanned);
	
	if (player.IsBanned && gB_GOKZLocalDB)
	{
		GOKZ_DB_SetCheater(client, true);
	}
}

static void LoadSounds()
{
	char downloadPath[PLATFORM_MAX_PATH];
	FormatEx(downloadPath, sizeof(downloadPath), "sound/%s", GL_SOUND_NEW_RECORD);
	AddFileToDownloadsTable(downloadPath);
	PrecacheSound(GL_SOUND_NEW_RECORD, true);
}
