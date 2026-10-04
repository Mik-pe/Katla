#+build darwin, arm64
//! Native fixtures verify mirrored tangent space, unlit properties and linear alpha blending.
package main

import app "../../app"
import render "../../app/render"
import ecs "../../ecs"
import gfx "../../gfx"
import km "../../math"
import "core:fmt"
import "core:mem"
import "core:path/filepath"

model_center_pixel :: proc(data:^gfx.Readback_Data,x,y:int)->[4]int {
    offset:=u64(y)*data.row_pitch+u64(x)*4
    return {int(data.bytes[offset]),int(data.bytes[offset+1]),int(data.bytes[offset+2]),int(data.bytes[offset+3])}
}
exercise_model_correctness :: proc(renderer:^$R,operations:render.GPU_Ops(R),capture:Capture_Ops(R),descriptor:gfx.Graphics_Desc,config:^render.Model_Config(R),backend,output,resource_path:string) {
    for path in ([]string{"models/MirrorNormal.gltf","models/UnlitBlend.gltf","models/TangentUV.gltf","models/BlendDepth.gltf"}) {
        owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner)
        assert(app.authoring_services_init(&owner)==.None)
        assert(app.asset_resources_init(&owner,filepath.dir(resource_path),resource_path)==.None)
        source,error:=app.scene_model_prepare(&owner,{path=path}); assert(error==.None)
        entity:=ecs.spawn(&owner.world,struct {model:app.Scene_Model,transform:app.Scene_Transform,surface:app.Surface_Material} {source,{km.TRANSFORM_IDENTITY},{metallic=1,roughness=1,ao=1}})
        consumer:render.Native_Consumer(R)
        assert(render.native_consumer_init(&consumer,&owner,renderer,operations,descriptor,3,256,256,models=config)=={})
        defer { assert(render.native_consumer_destroy(&consumer)==.None) }
        camera:=render.camera_default(); camera.position={0,0,4}
        frame,frame_error:=render.frame_data(camera,256,256,backend=="vulkan"); assert(frame_error==.None)
        before:=model_pixels(&consumer,capture,frame); defer gfx.readback_data_destroy(&before)
        filename:=fmt.aprintf("%s-%s.png",output,filepath.base(path)); defer delete(filename); save_pixels(&before,filename)
        center:=model_center_pixel(&before,128,128)
        if path=="models/MirrorNormal.gltf" {
            assert(center[0]>60)
            transform:=ecs.get_component_mut(&owner.world,entity,app.Scene_Transform); transform.local.scale={-1,1,1}
            mirrored:=model_pixels(&consumer,capture,frame); defer gfx.readback_data_destroy(&mirrored)
            actual:=model_center_pixel(&mirrored,128,128)
            for channel in 0..<3 { assert(abs(actual[channel]-center[channel])<=3,"mirrored tangent bitangent changed the actual normal-map lighting") }
            transform.local=km.TRANSFORM_IDENTITY
            model:=ecs.get_component_mut(&owner.world,entity,app.Scene_Model)
            model.model.nodes[0].local_matrix=km.transform_to_mat4(km.transform(scale={-1,1,1}))
            node_mirror:=model_pixels(&consumer,capture,frame); defer gfx.readback_data_destroy(&node_mirror)
            assert(mem.compare(mirrored.bytes,node_mirror.bytes)==0,"entity and node reflection disagree")
            transform.local.scale={-1,1,1}
            both:=model_pixels(&consumer,capture,frame); defer gfx.readback_data_destroy(&both)
            assert(mem.compare(before.bytes,both.bytes)==0,"two reflections failed to restore the original winding and handedness")
            mirrored_path:=fmt.aprintf("%s-MirrorNormal-reflected.png",output); defer delete(mirrored_path); save_pixels(&mirrored,mirrored_path)
            fmt.println("Native mirrored entity/node winding and tangent handedness PASS:",backend,center,actual)
        } else if path=="models/UnlitBlend.gltf" {
            clear:=[3]f32{9.0/255,10.0/255,13.0/255}
            background:=km.color_to_array(km.color_to_linear({clear[0],clear[1],clear[2],1}))
            encoded:=km.color_to_array(km.color_to_srgb(km.color_from_array(background*0.5+km.Vec4{0.25,0.25,0.25,0.5})))
            for channel in 0..<3 {
                expected:=encoded[channel]*255
                assert(abs(f32(center[channel])-expected)<1.5,"unlit alpha must blend linear base color and ignore emissive/normal/material properties")
            }
            assert(center[3]==255)
            fmt.println("Native linear unlit alpha blend and ignored non-base properties PASS:",backend,center)
        } else if path=="models/TangentUV.gltf" {
            left,right:=model_center_pixel(&before,79,128),model_center_pixel(&before,172,128)
            assert(left[0]>50 && right[0]>left[0]+8,"generated transformed-UV tangent and authored tangent must produce the expected distinct lighting")
            fmt.println("Native actual UV1 transformed generated tangent and retained authored tangent PASS:",backend,left,right)
        } else {
            assert(len(consumer.active.models.order)==2 && consumer.active.models.order[0]==1 && consumer.active.models.order[1]==0,"same-node transparent primitives must sort by actual camera-space bounds")
            background:=km.color_to_array(km.color_to_linear({9.0/255,10.0/255,13.0/255,1}))
            encoded:=km.color_to_array(km.color_to_srgb(km.color_from_array(background*0.25+km.Vec4{0.5,0,0.25,0.75})))
            for channel in 0..<3 { assert(abs(f32(center[channel])-encoded[channel]*255)<1.5,"transparent far-blue then near-red must compose in camera-depth order in linear color") }
            fmt.println("Native same-node primitive bounds camera-depth ordering and overlapping linear blend PASS:",backend,center)
        }
    }
}
