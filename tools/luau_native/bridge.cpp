#include "owner.hpp"
#include "lualib.h"
#include "Luau/Compiler.h"
#include <chrono>
#include <cstdlib>
#include <exception>
#include <string>
#include <thread>

#if defined(_WIN32)
#define KATLA_API extern "C" __declspec(dllexport)
#else
#define KATLA_API extern "C" __attribute__((visibility("default")))
#endif

static void* allocate(void* context,void* pointer,size_t before,size_t after) {
    Owner* value=static_cast<Owner*>(context);
    if (!after) { value->bytes-=before; std::free(pointer); return nullptr; }
    if (after>128*1024*1024 || value->bytes-before>128*1024*1024-after) return nullptr;
    void* result=std::realloc(pointer,after);
    if (result) value->bytes=value->bytes-before+after;
    return result;
}
static void interrupt(lua_State* state,int gc) {
    Owner* value=owner(state);
    if (gc >= 0 || !value->active) return;
    if (++value->instructions>10000000 || std::chrono::steady_clock::now()-value->start>=std::chrono::seconds(5))
        luaL_error(state,"Script execution budget exhausted");
}
KATLA_API uint32_t katla_luau_abi() { return 2; }
KATLA_API lua_State* katla_luau_create() {
    Owner* value=new(std::nothrow) Owner;
    if (!value) return nullptr;
    lua_State* state=lua_newstate(allocate,value);
    if (!state) { delete value; return nullptr; }
    lua_callbacks(state)->userdata=value; lua_callbacks(state)->interrupt=interrupt;
    bool initialized=protected_operation(state,[](lua_State* current,void*) {
    lua_pushliteral(current,"not enough memory"); owner(current)->memory_error=lua_ref(current,-1); lua_pop(current,1);
    luaL_openlibs(current);
    const char* stripped[]={"debug","io","package","require","dofile","loadfile"};
    for (const char* name:stripped) { lua_pushnil(current); lua_setglobal(current,name); }
    lua_getglobal(current,"os");
    if (lua_istable(current,-1)) {
        const char* dangerous[]={"execute","getenv","remove","rename","tmpname","exit"};
        for (const char* name:dangerous) { lua_pushnil(current); lua_setfield(current,-2,name); }
    }
    lua_pop(current,1);
    },nullptr);
    if (!initialized) { lua_close(state); delete value; return nullptr; }
    return state;
}
KATLA_API int katla_luau_owner_check(lua_State* state) { return state && owner(state)->thread==std::this_thread::get_id(); }
KATLA_API int katla_luau_destroy(lua_State* state) {
    if (!katla_luau_owner_check(state)) return 0;
    Owner* value=owner(state); lua_close(state); bool clean=value->bytes==0; delete value; return clean;
}
KATLA_API size_t katla_luau_bytes(lua_State* state) { return katla_luau_owner_check(state) ? owner(state)->bytes : 0; }
KATLA_API int katla_luau_compile_load(lua_State* state,const char* source,size_t count,const char* path,int environment) {
    if (!katla_luau_owner_check(state) || !source || count>1024*1024 || !path) return -1;
    if (owner(state)->pending) { int status=owner(state)->pending; push_pending_error(state); return status; }
    try {
        std::string code=Luau::compile(std::string(source,count));
        return luau_load(state,path,code.data(),code.size(),environment);
    } catch (const std::exception& error) {
        struct Message { const char* text; } message{error.what()};
        protected_operation(state,[](lua_State* current,void* opaque) { lua_pushstring(current,static_cast<Message*>(opaque)->text); },&message);
        if (owner(state)->pending) push_pending_error(state);
        return 1;
    }
}
KATLA_API int katla_luau_run(lua_State* state,int arguments,int results) {
    if (!katla_luau_owner_check(state)) return -1;
    Owner* value=owner(state); if (value->pending) { int status=value->pending; push_pending_error(state); return status; }
    bool nested=value->active;
    if (!nested) { value->instructions=0; value->start=std::chrono::steady_clock::now(); value->active=true; }
    int status=lua_pcall(state,arguments,results,0); if (!nested) value->active=false; return status;
}
using Callback=int(*)(lua_State*,void*);
static int invoke_callback(lua_State* state) {
    Callback callback=reinterpret_cast<Callback>(lua_tolightuserdata(state,lua_upvalueindex(1)));
    void* context=lua_tolightuserdata(state,lua_upvalueindex(2));
    int result=callback(state,context);
    if (owner(state)->pending) { push_pending_error(state); lua_error(state); }
    if (result<0) lua_error(state);
    return result;
}
KATLA_API void katla_luau_push_callback(lua_State* state,Callback callback,void* context,const char* name) {
    struct Arguments { Callback callback; void* context; const char* name; } arguments{callback,context,name};
    protected_operation(state,[](lua_State* current,void* opaque) {
        auto* value=static_cast<Arguments*>(opaque);
        lua_pushlightuserdata(current,reinterpret_cast<void*>(value->callback)); lua_pushlightuserdata(current,value->context);
        lua_pushcclosure(current,invoke_callback,value->name,2);
    },&arguments);
}
KATLA_API void katla_luau_sandbox(lua_State* state) { protected_operation(state,[](lua_State* current,void*) { luaL_sandbox(current); },nullptr); }
