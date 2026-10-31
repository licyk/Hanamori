#include <Windows.h>
#include <sdkddkver.h>
#include <cstdio>
#include <string>
#include <vector>

#ifndef _MSC_FULL_VER
#error This acceptance test requires the actual MSVC compiler.
#endif

#ifdef HANAMORI_MSBUILD
int linked_value();
#endif

static std::wstring environment(const wchar_t* name) {
    const DWORD size = GetEnvironmentVariableW(name, nullptr, 0);
    if (size == 0) return {};
    std::wstring value(size, L'\0');
    const DWORD length = GetEnvironmentVariableW(name, value.data(), size);
    if (length == 0 || length >= size) return {};
    value.resize(length);
    return value;
}

static std::string resource(WORD id, WORD type) {
    const HMODULE module = GetModuleHandleW(nullptr);
    const HRSRC entry = FindResourceW(module, MAKEINTRESOURCEW(id), MAKEINTRESOURCEW(type));
    if (!entry) return {};
    const HGLOBAL loaded = LoadResource(module, entry);
    if (!loaded) return {};
    const auto data = static_cast<const char*>(LockResource(loaded));
    if (!data) return {};
    return std::string(data, SizeofResource(module, entry));
}

int wmain(int argc, wchar_t** argv) {
    if (argc != 4) return 10;
#ifdef HANAMORI_MSBUILD
    if (linked_value() != 42) return 17;
#endif
    if (environment(L"VCToolsVersion") != argv[1]) return 11;
    if (environment(L"WindowsSDKVersion") != argv[2]) return 12;
    if (environment(L"_HANAMORI_ACTIVE_ID") != argv[3]) return 13;
    const std::vector<int> values{1, 2, 3};
    if (GetCurrentProcessId() == 0 || values.size() != 3) return 14;
    if (resource(101, 10) != "hanamori-sdk-resource") return 15;
    if (resource(1, 24).find("Hanamori.NativeAcceptance") == std::string::npos) return 16;
    std::printf("hanamori-native-build %d\n", _MSC_FULL_VER);
    return 0;
}
