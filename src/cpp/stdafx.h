#pragma once

#include "targetver.h"

#define WIN32_LEAN_AND_MEAN             // Exclude rarely-used stuff from Windows headers

// Windows Header Files
#include <windows.h>
#include <atlbase.h>
#include <atlcom.h>
#include <UIAutomation.h>
#include <strsafe.h>
#include <map>
#include <string>
#include <regex>

// Additional headers needed for the project
#include <oleacc.h>
#include <windowsx.h>

// Common macros
#define REQUIRE_SUCCESS_HR(hr) { HRESULT _hr = (hr); if (FAILED(_hr)) { return _hr; } }