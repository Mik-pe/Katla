#include "owner.hpp"
#include "lualib.h"
#include "lstate.h"
#include "ldo.h"
#include <algorithm>
#include <cstring>

#if defined(_WIN32)
#define KATLA_API extern "C" __declspec(dllexport)
#else
#define KATLA_API extern "C" __attribute__((visibility("default")))
#endif

bool protected_operation(lua_State* state,Operation operation,void* data) {
    if (!state || owner(state)->thread!=std::this_thread::get_id() || owner(state)->pending) return false;
    Owner* value=owner(state);
    int top=lua_gettop(state);
    bool nested=value->active;
    if (!nested) { value->instructions=0; value->start=std::chrono::steady_clock::now(); value->active=true; }
    int status=luaD_pcall(state,operation,data,savestack(state,state->top),0);
    if (!nested) value->active=false;
    if (status==0) return true;
    const char* message="Native Luau operation failed";
    size_t count=std::strlen(message);
    if (lua_type(state,-1)==LUA_TSTRING) message=lua_tolstring(state,-1,&count);
    count=std::min(count,sizeof(value->error)-1);
    std::memcpy(value->error,message,count); value->error[count]=0; value->pending=status;
    lua_settop(state,top);
    return false;
}

template<typename Function>
static bool protect(lua_State* state,Function function) {
    return protected_operation(state,[](lua_State* current,void* opaque) { (*static_cast<Function*>(opaque))(current); },&function);
}
void push_pending_error(lua_State* state) {
    Owner* value=owner(state); char message[sizeof(value->error)]; std::memcpy(message,value->error,sizeof(message)); value->pending=0;
    if (!protect(state,[&](lua_State* current) { lua_pushstring(current,message); })) {
        value->pending=0; lua_rawgeti(state,LUA_REGISTRYINDEX,value->memory_error);
    }
}
KATLA_API const char* katla_luau_take_error(lua_State* state) {
    if (!state || owner(state)->thread!=std::this_thread::get_id()) return "Script runtime thread mismatch";
    Owner* value=owner(state); if (!value->pending) return nullptr;
    value->pending=0; return value->error;
}
KATLA_API void katla_luau_settop(lua_State* state,int index) { protect(state,[&](lua_State* current) { lua_settop(current,index); }); }
KATLA_API void katla_luau_pushvalue(lua_State* state,int index) { protect(state,[&](lua_State* current) { lua_pushvalue(current,index); }); }
KATLA_API void katla_luau_pushnil(lua_State* state) { protect(state,[](lua_State* current) { lua_pushnil(current); }); }
KATLA_API void katla_luau_pushnumber(lua_State* state,double value) { protect(state,[&](lua_State* current) { lua_pushnumber(current,value); }); }
KATLA_API void katla_luau_pushboolean(lua_State* state,int value) { protect(state,[&](lua_State* current) { lua_pushboolean(current,value); }); }
KATLA_API void katla_luau_pushlstring(lua_State* state,const char* value,size_t count) { protect(state,[&](lua_State* current) { lua_pushlstring(current,value,count); }); }
KATLA_API const char* katla_luau_tolstring(lua_State* state,int index,size_t* count) { const char* result=nullptr; protect(state,[&](lua_State* current) { result=lua_tolstring(current,index,count); }); return result; }
KATLA_API void* katla_luau_newuserdatatagged(lua_State* state,size_t count,int tag) { void* result=nullptr; protect(state,[&](lua_State* current) { result=lua_newuserdatatagged(current,count,tag); }); return result; }
KATLA_API void* katla_luau_newuserdatadtor(lua_State* state,size_t count,void(*destroy)(void*)) { void* result=nullptr; protect(state,[&](lua_State* current) { result=lua_newuserdatadtor(current,count,destroy); }); return result; }
KATLA_API void katla_luau_createtable(lua_State* state,int array,int records) { protect(state,[&](lua_State* current) { lua_createtable(current,array,records); }); }
KATLA_API int katla_luau_getfield(lua_State* state,int index,const char* field) { int result=LUA_TNONE; protect(state,[&](lua_State* current) { result=lua_getfield(current,index,field); }); return result; }
KATLA_API void katla_luau_setfield(lua_State* state,int index,const char* field) { protect(state,[&](lua_State* current) { lua_setfield(current,index,field); }); }
KATLA_API int katla_luau_rawgeti(lua_State* state,int index,int key) { int result=LUA_TNONE; protect(state,[&](lua_State* current) { result=lua_rawgeti(current,index,key); }); return result; }
KATLA_API void katla_luau_rawseti(lua_State* state,int index,int key) { protect(state,[&](lua_State* current) { lua_rawseti(current,index,key); }); }
KATLA_API int katla_luau_setmetatable(lua_State* state,int index) { int result=0; protect(state,[&](lua_State* current) { result=lua_setmetatable(current,index); }); return result; }
KATLA_API int katla_luau_setfenv(lua_State* state,int index) { int result=0; protect(state,[&](lua_State* current) { result=lua_setfenv(current,index); }); return result; }
KATLA_API int katla_luau_ref(lua_State* state,int index) { int result=-1; protect(state,[&](lua_State* current) { result=lua_ref(current,index); }); return result; }
KATLA_API void katla_luau_unref(lua_State* state,int reference) { protect(state,[&](lua_State* current) { lua_unref(current,reference); }); }
KATLA_API void katla_luau_setreadonly(lua_State* state,int index,int value) { protect(state,[&](lua_State* current) { lua_setreadonly(current,index,value); }); }
KATLA_API int katla_luau_next(lua_State* state,int index) { int result=0; protect(state,[&](lua_State* current) { result=lua_next(current,index); }); return result; }
KATLA_API int katla_luau_gc(lua_State* state,int operation,int count) { int result=-1; protect(state,[&](lua_State* current) { result=lua_gc(current,operation,count); }); return result; }
KATLA_API void katla_luau_rawsetfield(lua_State* state,int index,const char* field) { protect(state,[&](lua_State* current) { lua_rawsetfield(current,index,field); }); }

KATLA_API void katla_luau_remove(lua_State* state,int index) { protect(state,[&](lua_State* current) { lua_remove(current,index); }); }
KATLA_API void katla_luau_insert(lua_State* state,int index) { protect(state,[&](lua_State* current) { lua_insert(current,index); }); }
