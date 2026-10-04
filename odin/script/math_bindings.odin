//! Native userdata preserves the public script math types and their f32 value semantics.
package script
import luau "../deps/luau"
import km "../math"
import "core:strings"
import "core:fmt"
import "core:math"

@(private="package")
binding_error :: proc(owner:^Runtime,message:string)->i32 { luau.push_string(&owner.vm,message); return -1 }
@(private="package")
bind :: proc(owner:^Runtime,name:string,callback:luau.Callback,field:cstring) {
    assert(owner.binding_count<len(owner.bindings)); value:=&owner.bindings[owner.binding_count]; owner.binding_count+=1; value^={owner,strings.clone(name,owner.allocator)}
    owner.vm.api.callback(owner.vm.state,callback,value,"Katla"); owner.vm.api.set_field(owner.vm.state,-2,field)
}
@(private="package")
userdata :: proc(owner:^Runtime,index,tag:i32)->rawptr {
    api,s:=owner.vm.api,owner.vm.state
    if api.type(s,index)!=.Userdata || api.userdata_tag(s,index)!=tag { return nil }; return api.userdata(s,index)
}
@(private="package")
push_typed :: proc(owner:^Runtime,value:$T,tag:i32) { vm:=&owner.vm; pointer:=cast(^T)vm.api.new_userdata(vm.state,uint(size_of(T)),tag); if pointer==nil { return }; pointer^=value; luau.get_reference(vm,owner.metatables[tag]); vm.api.set_metatable(vm.state,-2) }
@(private="package")
number :: proc(owner:^Runtime,index:i32)->(f32,bool) { valid:i32; value:=owner.vm.api.number(owner.vm.state,index,&valid); return f32(value),valid!=0 }
@(private="package")
vector :: proc(owner:^Runtime,index:i32)->(km.Vec3,bool) {
    if pointer:=cast(^km.Vec3)userdata(owner,index,1); pointer!=nil { return pointer^,true }
    if owner.vm.api.type(owner.vm.state,index)!=.Table { return {},false }
    result:km.Vec3; absolute:=owner.vm.api.abs_index(owner.vm.state,index)
    for field,i in ([3]cstring{"x","y","z"}) { owner.vm.api.get_field(owner.vm.state,absolute,field); if owner.vm.api.type(owner.vm.state,-1)==.Nil { luau.pop(&owner.vm); owner.vm.api.raw_get_i(owner.vm.state,absolute,i32(i+1)) }; value,valid:=number(owner,-1); luau.pop(&owner.vm); if !valid { return {},false }; result[i]=value }
    return result,true
}
@(private="file")
math_callback :: proc "c"(state:luau.State,opaque:rawptr)->i32 {
    binding:=cast(^Binding)opaque; owner:=binding.owner; context=owner.ctx; vm:=&owner.vm; api:=vm.api; op:=binding.name
    switch op {
    case "Vec3.new","Quat.new","Color.new","Color.rgb":
        count:=3; if op=="Quat.new" || op=="Color.new" { count=4 }; values:km.Vec4
        for i in 0..<count { value,valid:=number(owner,i32(i+1)); if !valid { return binding_error(owner,"Math constructor requires numbers") }; values[i]=value }
        if op=="Vec3.new" { push_typed(owner,km.xyz(values),1) } else if op=="Quat.new" { push_typed(owner,km.Quat(values),2) } else { if count==3 { values[3]=1 }; push_typed(owner,values,4) }; return 1
    case "Quat.from_axis_angle":
        axis,valid:=vector(owner,1); angle,angle_valid:=number(owner,2); if !valid || !angle_valid { return binding_error(owner,"Axis and angle required") }; push_typed(owner,km.quat_axis_angle(axis,angle),2); return 1
    case "Quat.identity": push_typed(owner,km.QUAT_IDENTITY,2); return 1
    case "Color.from_rgb_hex":
        numeric:i32; value:=api.number(state,1,&numeric); if numeric==0 || math.is_nan(value) || math.is_inf(value) || value<0 || value>f64(max(u32)) || value!=math.floor(value) { return binding_error(owner,"Hex color requires u32") }; hex:=u32(value); push_typed(owner,km.Vec4{f32((hex>>16)&255)/255,f32((hex>>8)&255)/255,f32(hex&255)/255,1},4); return 1
    }
    tag:=api.userdata_tag(state,1); pointer:=userdata(owner,1,tag); if pointer==nil || tag<1 || tag>4 { return binding_error(owner,"Math userdata required") }
    if op=="__index" || op=="__newindex" {
        field:=luau.to_string(vm,2)
        if tag==3 {
            target:=cast(^km.Transform)pointer
            if op=="__index" {
                switch field {
                case "position": push_typed(owner,target.position,1); return 1
                case "scale": push_typed(owner,target.scale,1); return 1
                case "rotation": push_typed(owner,target.rotation,2); return 1
                }
            } else {
                if field=="rotation" { value:=cast(^km.Quat)userdata(owner,3,2); if value==nil { return binding_error(owner,"Rotation requires Quat") }; target.rotation=value^; return 0 }
                value,valid:=vector(owner,3); if !valid { return binding_error(owner,"Transform field requires Vec3") }; if field=="position" { target.position=value; return 0 }; if field=="scale" { target.scale=value; return 0 }; return binding_error(owner,"Unknown transform field")
            }
        } else {
            slot:=-1; labels:="xyzw"; if tag==4 { labels="rgba" }; if len(field)==1 { for label,i in labels { if u8(label)==field[0] { slot=i; break } } }
            if slot>=0 && (tag!=1 || slot<3) { values:=cast([^]f32)pointer; if op=="__index" { api.push_number(state,f64(values[slot])); return 1 }; if tag==2 { return binding_error(owner,"Quat fields are read-only") }; value,valid:=number(owner,3); if !valid { return binding_error(owner,"Math field requires number") }; values[slot]=value; return 0 }
            if op=="__newindex" { return binding_error(owner,"Unknown math field") }
        }
        luau.get_reference(vm,owner.metatables[tag]); key:=strings.clone_to_cstring(field,owner.allocator); api.get_field(state,-1,key); delete(key,owner.allocator); api.remove(state,-2); return 1
    }
    if tag==1 {
        a:=(cast(^km.Vec3)pointer)^; b,b_valid:=vector(owner,2); scalar,scalar_valid:=number(owner,2); result:km.Vec3
        switch op {
        case "length": api.push_number(state,f64(km.length(a))); return 1
        case "length_squared": api.push_number(state,f64(km.length_squared(a))); return 1
        case "normalize","normalized": result=km.normalize(a)
        case "__unm": result=-a
        case "__tostring": value:=fmt.aprintf("Vec3(%g, %g, %g)",a[0],a[1],a[2]); luau.push_string(vm,value); delete(value); return 1
        case "__mul": if scalar_valid { result=a*scalar } else if b_valid { result=a*b } else { return binding_error(owner,"Vec3 multiply requires Vec3 or number") }
        case "__div": if !scalar_valid || scalar==0 { return binding_error(owner,"Vec3 division requires nonzero number") }; result=a/scalar
        case:
            if !b_valid { return binding_error(owner,"Vec3 argument required") }
            switch op {
            case "__add": result=a+b
            case "__sub": result=a-b
            case "dot": api.push_number(state,f64(km.dot(a,b))); return 1
            case "cross": result=km.cross(a,b)
            case "distance": api.push_number(state,f64(km.distance(a,b))); return 1
            case "lerp": factor,valid:=number(owner,3); if !valid { return binding_error(owner,"Lerp factor required") }; result=km.lerp(a,b,factor)
            case: return binding_error(owner,"Unknown Vec3 method")
            }
        }
        push_typed(owner,result,1); return 1
    }
    if tag==2 {
        a:=(cast(^km.Quat)pointer)^; b:=cast(^km.Quat)userdata(owner,2,2); result:km.Quat
        switch op {
        case "conjugate": result=km.quat_conjugate(a)
        case "normalize": result=km.quat_normalize(a)
        case "__mul":
            if !km.quat_is_normalized(a) { return binding_error(owner,"Quaternion rotation requires unit quaternion") }
            if b!=nil { if !km.quat_is_normalized(b^) { return binding_error(owner,"Quaternion rotation requires unit quaternion") }; result=km.quat_mul(a,b^) } else { value,valid:=vector(owner,2); if !valid { return binding_error(owner,"Quat multiply requires Quat or Vec3") }; push_typed(owner,km.quat_transform_vector(a,value),1); return 1 }
        case "slerp": factor,valid:=number(owner,3); if b==nil || !valid { return binding_error(owner,"Slerp requires Quat and number") }; result=km.quat_slerp(a,b^,factor)
        case "__tostring": value:=fmt.aprintf("Quat(%g, %g, %g, %g)",a[0],a[1],a[2],a[3]); luau.push_string(vm,value); delete(value); return 1
        case: return binding_error(owner,"Unknown Quat method")
        }
        push_typed(owner,result,2); return 1
    }
    if tag==3 {
        a:=(cast(^km.Transform)pointer)^; if !km.quat_is_normalized(a.rotation) { return binding_error(owner,"Transform rotation requires unit quaternion") }
        switch op {
        case "forward": push_typed(owner,km.transform_forward(a),1); return 1
        case "up": push_typed(owner,km.quat_transform_vector(a.rotation,km.VEC3_Y),1); return 1
        case "right": push_typed(owner,km.quat_transform_vector(a.rotation,km.VEC3_X),1); return 1
        case "look_at": target,valid:=vector(owner,2); if !valid { return binding_error(owner,"Look-at target requires Vec3") }; push_typed(owner,km.transform_look_at(a,target,km.VEC3_Y),3); return 1
        case "lerp": other:=cast(^km.Transform)userdata(owner,2,3); factor,valid:=number(owner,3); if other==nil || !valid { return binding_error(owner,"Transform lerp requires Transform and number") }; push_typed(owner,km.transform_lerp(a,other^,factor),3); return 1
        case "inverse": rotation:=km.quat_conjugate(a.rotation); scale:km.Vec3; for value,i in a.scale { if value!=0 { scale[i]=1/value } }; push_typed(owner,km.Transform{km.quat_transform_vector(rotation,-a.position)*scale,scale,rotation},3); return 1
        }
    }
    if tag==4 {
        a:=(cast(^km.Vec4)pointer)^; other:=cast(^km.Vec4)userdata(owner,2,4); factor,valid:=number(owner,2); result:km.Vec4
        switch op {
        case "with_alpha": if !valid { return binding_error(owner,"Alpha requires number") }; result=a; result[3]=factor
        case "clamped": for value,i in a { result[i]=clamp(value,0,1) }
        case "__mul": if !valid { return binding_error(owner,"Color multiply requires number") }; result=a*factor
        case "__add","__sub","lerp": if other==nil { return binding_error(owner,"Color argument required") }; if op=="__add" { result=a+other^ } else if op=="__sub" { result=a-other^ } else { amount,amount_valid:=number(owner,3); if !amount_valid { return binding_error(owner,"Lerp factor required") }; result=km.lerp(a,other^,amount) }
        case "__tostring": value:=fmt.aprintf("Color(%g, %g, %g, %g)",a[0],a[1],a[2],a[3]); luau.push_string(vm,value); delete(value); return 1
        case: return binding_error(owner,"Unknown Color method")
        }
        push_typed(owner,result,4); return 1
    }
    return binding_error(owner,"Unknown math method")
}
@(private="package")
register_math :: proc(owner:^Runtime) {
    vm:=&owner.vm
    for tag in i32(1)..<i32(5) {
        vm.api.create_table(vm.state,0,24)
        methods:[]cstring
        switch tag {
        case 1: methods={"length","length_squared","normalize","normalized","dot","cross","lerp","distance","__add","__sub","__mul","__div","__unm","__tostring"}
        case 2: methods={"conjugate","normalize","slerp","__mul","__tostring"}
        case 3: methods={"forward","up","right","look_at","lerp","inverse"}
        case 4: methods={"with_alpha","lerp","clamped","__add","__sub","__mul","__tostring"}
        }
        for name in methods { bind(owner,string(name),math_callback,name) }; bind(owner,"__index",math_callback,"__index"); bind(owner,"__newindex",math_callback,"__newindex")
        lock_metatable(owner)
        owner.metatables[tag]=vm.api.reference(vm.state,-1); luau.pop(vm)
    }
    for name,tag in ([4]cstring{"Vec3","Quat","Transform","Color"}) {
        vm.api.create_table(vm.state,0,8)
        if tag!=2 { constructor:=strings.concatenate({string(name),".new"},owner.allocator); bind(owner,constructor,math_callback,"new"); delete(constructor,owner.allocator) }
        switch tag {
        case 0: push_typed(owner,km.VEC3_ZERO,1); vm.api.set_field(vm.state,-2,"zero")
        case 1: bind(owner,"Quat.from_axis_angle",math_callback,"from_axis_angle"); bind(owner,"Quat.identity",math_callback,"identity")
        case 3: bind(owner,"Color.rgb",math_callback,"rgb"); bind(owner,"Color.from_rgb_hex",math_callback,"from_rgb_hex")
        }
        vm.api.set_field(vm.state,luau.GLOBALS_INDEX,name)
    }
}

@(private="package")
lock_metatable :: proc(owner:^Runtime) { luau.push_string(&owner.vm,"Katla native userdata"); owner.vm.api.set_field(owner.vm.state,-2,"__metatable"); owner.vm.api.readonly(owner.vm.state,-1,1) }
