#pragma once
#include "lua.h"
#include <chrono>
#include <thread>
#include <cstdint>

struct Owner {
    std::thread::id thread=std::this_thread::get_id();
    size_t bytes=0;
    uint64_t instructions=0;
    std::chrono::steady_clock::time_point start;
    bool active=false;
    int pending=0;
    int memory_error=0;
    char error[8193]{};
};
inline Owner* owner(lua_State* state) { return static_cast<Owner*>(lua_callbacks(state)->userdata); }
using Operation=void(*)(lua_State*,void*);
bool protected_operation(lua_State*,Operation,void*);
void push_pending_error(lua_State*);
