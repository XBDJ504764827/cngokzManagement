#if defined _STEAMWORKS_MINIMAL_INC_
    #endinput
#endif
#define _STEAMWORKS_MINIMAL_INC_

#pragma semicolon 1
#pragma newdecls required

enum EHTTPMethod
{
    k_EHTTPMethodInvalid = 0,
    k_EHTTPMethodGET,
    k_EHTTPMethodHEAD,
    k_EHTTPMethodPOST,
    k_EHTTPMethodPUT,
    k_EHTTPMethodDELETE,
    k_EHTTPMethodOPTIONS,
    k_EHTTPMethodPATCH
};

enum EHTTPStatusCode
{
    k_EHTTPStatusCodeInvalid = 0,
    k_EHTTPStatusCode200OK = 200,
    k_EHTTPStatusCode401Unauthorized = 401,
    k_EHTTPStatusCode404NotFound = 404,
    k_EHTTPStatusCode500InternalServerError = 500
};

typeset SteamWorksHTTPRequestCompleted
{
    function int (Handle request, bool failure, bool requestSuccessful, EHTTPStatusCode statusCode);
    function int (Handle request, bool failure, bool requestSuccessful, EHTTPStatusCode statusCode, any contextData);
    function int (Handle request, bool failure, bool requestSuccessful, EHTTPStatusCode statusCode, any contextData1, any contextData2);
};

typeset SteamWorksHTTPHeadersReceived
{
    function int (Handle request);
    function int (Handle request, any contextData);
    function int (Handle request, any contextData1, any contextData2);
};

typeset SteamWorksHTTPDataReceived
{
    function int (Handle request, int offset, int bytesReceived);
    function int (Handle request, int offset, int bytesReceived, any contextData);
    function int (Handle request, int offset, int bytesReceived, any contextData1, any contextData2);
};

native Handle SteamWorks_CreateHTTPRequest(EHTTPMethod method, const char[] url);
native bool SteamWorks_SetHTTPRequestContextValue(Handle request, any contextValue1, any contextValue2=0);
native bool SteamWorks_SetHTTPRequestNetworkActivityTimeout(Handle request, int timeoutSeconds);
native bool SteamWorks_SetHTTPRequestHeaderValue(Handle request, const char[] headerName, const char[] headerValue);
native bool SteamWorks_SetHTTPRequestGetOrPostParameter(Handle request, const char[] paramName, const char[] paramValue);
native bool SteamWorks_SetHTTPCallbacks(
    Handle request,
    SteamWorksHTTPRequestCompleted completedCallback=INVALID_FUNCTION,
    SteamWorksHTTPHeadersReceived headersCallback=INVALID_FUNCTION,
    SteamWorksHTTPDataReceived dataCallback=INVALID_FUNCTION,
    Handle callingPlugin=INVALID_HANDLE
);
native bool SteamWorks_SendHTTPRequest(Handle request);
native bool SteamWorks_GetHTTPResponseBodySize(Handle request, int &size);
native bool SteamWorks_GetHTTPResponseBodyData(Handle request, char[] bodyData, int size);
