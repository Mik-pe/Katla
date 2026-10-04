#+build darwin, arm64
//! Actual scene geometry proves lighting, shadows, overlays and the final HDR transform.
package main

import app "../../app"
import render "../../app/render"
import ecs "../../ecs"
import gfx "../../gfx"
import metal "../../gfx/metal"
import vulkan "../../gfx/vulkan"
import km "../../math"
import shader "../../gfx/shader"
import "core:fmt"
import "core:mem"
import "core:os"
import "core:time"
import NS "core:sys/darwin/Foundation"

Capture :: struct($R:typeid) { source:proc(^R,gfx.Submission,gfx.Image_Id)->(gfx.Texture_Source,gfx.Gpu_Error),queue:proc(^R,gfx.Texture_Source,gfx.Image_Region)->(gfx.Readback_Ticket,gfx.Gpu_Error),poll:proc(^R,gfx.Readback_Ticket)->(gfx.Readback_Data,bool,gfx.Gpu_Error) }
capture :: proc(scene:^render.Native_Scene($R),ops:Capture(R),submission:gfx.Submission,id:gfx.Image_Id,aspect:gfx.Image_Aspect=.Color)->gfx.Readback_Data {
    source,error:=ops.source(scene.renderer,submission,id); assert(error==.None)
    ticket,queue_error:=ops.queue(scene.renderer,source,{width=source.desc.width,height=source.desc.height,depth=1,aspect=aspect}); assert(queue_error==.None)
    start:=time.tick_now()
    for { result,ready,poll_error:=ops.poll(scene.renderer,ticket); assert(poll_error==.None); if ready { return result }; assert(time.tick_since(start)<10*time.Second); time.sleep(time.Millisecond) }
}
changed_pixels :: proc(a,b:^gfx.Readback_Data,threshold:int)->int {
    assert(len(a.bytes)==len(b.bytes)); changed:=0
    for offset:=0;offset<len(a.bytes);offset+=4 { difference:=0; for channel in 0..<3 { difference+=abs(int(a.bytes[offset+channel])-int(b.bytes[offset+channel])) }; if difference>threshold { changed+=1 } }
    return changed
}
spawn_mesh :: proc(owner:^app.Authoring,source:string,position:km.Vec3,color:km.Color)->ecs.Entity_Id {
    mesh,error:=app.scene_mesh_prepare(owner,{kind=.Geometry,geometry=transmute([]byte)source}); assert(error==.None)
    return ecs.spawn(&owner.world,struct {transform:app.Scene_Transform,mesh:app.Scene_Mesh,material:app.Surface_Material}{{km.transform(position=position)},mesh,{color,true,0,0.7,1}})
}
exercise :: proc(renderer:^$R,ops:render.GPU_Ops(R),overlay_ops:render.UI_GPU_Ops(R),captures:Capture(R),pipelines:render.Scene_Pipelines,backend:string,compiled:^render.Overlay_Shader,fonts:^render.UI_Font_System,pick_shader:^render.Picking_Shader,box_library:string) {
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner); assert(app.authoring_services_init(&owner)==.None)
    light:=ecs.spawn(&owner.world,struct {transform:app.Scene_Transform,light:app.Scene_Point_Light}{{km.transform(position={-1.4,1.7,0})},{{1,1,1},0,1}})
    assert(light==0,"native billboard acceptance must include the initial valid zero entity")
    floor:=spawn_mesh(&owner,`{"kind":"plane","width":8,"height":8}`,{0,0,0},{0.1,0.1,0.1,1})
    cube:=spawn_mesh(&owner,`{"kind":"cube","size":[1,1,1]}`,{0,.5,0},{.2,.2,.2,1})
    // The visual floor is y=0; its actual collider top is also y=0 after this authored placement.
    physics_ground:=ecs.spawn(&owner.world,struct {transform:app.Scene_Transform,body:app.Physics_Body}{{km.transform(position={0,-.5,0})},app.physics_body({kind=.Box,half_extents={4,.5,4}},.Fixed)})
    ball:=ecs.spawn(&owner.world,struct {transform:app.Scene_Transform,body:app.Physics_Body}{{km.transform(position={1.5,2,0})},app.physics_body({kind=.Sphere,radius=.5})})
    _=ecs.spawn(&owner.world,struct {transform:app.Scene_Transform,body:app.Physics_Body}{{km.transform(position={-1.4,.6,-1})},app.physics_body({kind=.Capsule,radius=.2,half_height=.4},.Fixed,true)})
    _=ecs.spawn(&owner.world,struct {transform:app.Scene_Transform,zone:app.Reverb_Zone}{{km.transform(position={-.8,.7,0})},{.7,.4,.3,{.5,.7,.5}}})
    hull:=spawn_mesh(&owner,`{"kind":"cube","size":[0.5,0.5,0.5]}`,{-1.8,.3,1},{.12,.12,.12,1})
    ecs.add_component(&owner.world,hull,app.physics_body({kind=.ConvexHull},.Fixed))
    authored:=ecs.spawn(&owner.world,struct {transform:app.Scene_Transform,billboard:app.Scene_Billboard}{{km.transform(position={-.2,2.3,0})},{.Fire,{.5,.25,.75,1},1.5}})
    _=ecs.spawn(&owner.world,struct {transform:app.Scene_Transform,emitter:app.Particle_Emitter}{{km.transform(position={1.4,1.7,0})},{app.particle_defaults()}})
    _=ecs.spawn(&owner.world,struct {transform:app.Scene_Transform,billboard:app.Scene_Billboard}{{km.transform(position={0,1.9,1})},{.Lightbulb,{1,1,1,.005},2}})
    _=floor
    _=ecs.spawn(&owner.world,struct {sun:app.Scene_Directional_Light}{{{-1,-1,-1},{1,1,1},2}})
    assert(app.physics_select_box3d(&owner,box_library)==.None)
    for _ in 0..<180 { step:=app.physics_step(&owner,1.0/60); if step.error!=.None { fmt.println("Overlay native physics failed:",step.error) }; assert(step.error==.None); app.physics_step_result_destroy(&step) }
    edges,edge_error:=app.physics_collider_edges(&owner,hull); assert(edge_error==.None && len(edges)==12); delete(edges)
    native_contacts,contact_error:=app.physics_contacts(&owner); assert(contact_error==.None && len(native_contacts)>0); defer delete(native_contacts)
    contacts:=make([]render.Overlay_Contact,len(native_contacts)); defer delete(contacts)
    for contact,i in native_contacts { assert((contact.a==physics_ground && contact.b==ball) || (contact.a==ball && contact.b==physics_ground)); contacts[i]={contact.a,contact.b,contact.point,contact.normal} }
    batch,batch_error:=render.scene_batch_prepare(&owner); assert(batch_error=={}); defer render.scene_batch_destroy(&batch)
    overlay:render.Overlay_Native(R); init_error:=render.overlay_native_init(&overlay,renderer,overlay_ops,compiled,fonts); if init_error!={} { fmt.println("Overlay init:",backend,init_error) }; assert(init_error=={}); defer assert(render.overlay_native_destroy(&overlay)==.None)
    masked,masked_error:=ops.create_pipeline(renderer,pick_shader.mapped.descriptor); assert(masked_error==.None); defer assert(ops.destroy_pipeline(renderer,masked)==.None)
    views:[4]render.Native_Scene(R)
    for &view in views {
        assert(render.native_scene_init(&view,renderer,ops,pipelines,&batch.geometry,len(batch.objects),3,256,192)=={})
        view.authoring=&owner; view.batch=&batch; view.feature_settings.grid=false; view.feature_settings.sky=false; view.feature_settings.shadows=false; view.feature_settings.outline=false; view.feature_settings.shadow_size=256
        assert(render.native_scene_resize(&view,256,192)=={})
    }
    defer { for &view in views { assert(render.native_scene_destroy(&view)==.None) } }
    graph:gfx.Graph; gfx.graph_init(&graph); defer gfx.graph_destroy(&graph)
    names:=[4]string{"Perspective","Front","Side","Top"}
    positions:=[4]km.Vec3{{0,2.6,5},{0,1.3,5},{5,1.3,.01},{0,5,.01}}
    frames:[4]render.Frame_Data
    for position,i in positions { camera:=render.camera_default(); camera.position=position; camera.target={0,.6,0}; frames[i],_=render.frame_data(camera,256,192,backend=="vulkan") }
    baseline:[4]gfx.Readback_Data; physical:[4]gfx.Readback_Data; translated:[4]gfx.Readback_Data
    defer { for &data in baseline { gfx.readback_data_destroy(&data) }; for &data in physical { gfx.readback_data_destroy(&data) }; for &data in translated { gfx.readback_data_destroy(&data) } }
    checked:=0
    for variation in ([9]int{0,1,2,3,5,6,7,8,4}) {
        assert(ops.release_exports(renderer,&graph)==.None); assert(gfx.graph_truncate(&graph,0,0,0)==.None)
        token,acquire_error:=ops.acquire(renderer); assert(acquire_error==.None)
        prepared:[4]render.Native_Prepared(R); uploads:[4]render.Overlay_Frame
        buffers:=make([dynamic]gfx.Buffer_Input); defer delete(buffers); textures:=make([dynamic]gfx.Texture_Input); defer delete(textures)
        imports:render.Overlay_Graph
        changes:[4]int
        pick_ids:[4]gfx.Image_Id; authored_codes:[4]u32
        pick_handles:[4][2]gfx.Texture_Handle
        defer { for handles in pick_handles { for handle in handles { if handle.owner!=nil { assert(ops.destroy_texture(renderer,handle)==.None) } } } }
        for &view,i in views {
            state:=render.Editor_Overlay_State{selected={cube},pivot={0,.5,0},basis=km.identity(km.Mat4)}
            switch variation {
            case 0:
            case 1: state.physics=true
            case 2: state.physics=true; state.contacts=contacts
            case 3: state.reverb=true
            case 4: state.billboards=true
            case 5: state.gizmo=true; state.mode=.Translate
            case 6: state.gizmo=true; state.mode=.Rotate
            case 7: state.gizmo=true; state.mode=.Scale
            case 8: state.gizmo=true; state.mode=.Translate; state.hover=.Axis_X; state.captured=.Plane_YZ
            }
            mesh,mesh_error:=render.overlay_mesh_prepare(&owner,state,frames[i],256,192); if mesh_error!=.None { fmt.println("Overlay mesh:",backend,i,variation,mesh_error) }; assert(mesh_error==.None); defer render.overlay_mesh_destroy(&mesh)
            error:render.Native_Error
            prepared[i],error=render.native_scene_prepare(&view,token,frames[i],batch.objects,batch.draws,destination=&graph,namespace=names[i]); assert(error=={})
            uploads[i],error=render.overlay_native_append(&overlay,&prepared[i],&mesh,frames[i],&imports); if error!={} { fmt.println("Overlay append:",backend,i,variation,error) }; assert(error=={})
            append(&buffers,..prepared[i].buffers[:]); append(&textures,..prepared[i].textures[:])
            if variation==4 {
                entries:=[1]render.Picking_Entry{{1,cube}}
                picking_draws,pick_error:=render.overlay_picking_draws(&overlay,&uploads[i],&mesh,&view.graph,&imports,entries[:],masked); assert(pick_error=={} && len(picking_draws)==4); defer delete(picking_draws)
                for draw in picking_draws { if draw.entity==authored { authored_codes[i]=draw.encoded } }; assert(authored_codes[i]!=0)
                graph.images[view.graph.color.index].exported=true; graph.revision+=1
                id_desc:=gfx.Texture_Desc{width=256,height=192,depth=1,layers=1,mip_levels=1,format=.R32_Uint,usage={.Color_Attachment,.Transfer_Source}}
                depth_desc:=gfx.Texture_Desc{width=256,height=192,depth=1,layers=1,mip_levels=1,format=.D32_Float,usage={.Depth_Attachment}}
                pick_handles[i][0],_=ops.create_texture(renderer,id_desc); pick_handles[i][1],_=ops.create_texture(renderer,depth_desc)
                pick_ids[i],_=gfx.graph_image(&graph,id_desc,{initial=.Undefined,final=.Color_Attachment},false,true)
                pick_depth,_:=gfx.graph_image(&graph,depth_desc,{initial=.Undefined,final=.Depth_Attachment},false,false)
                pick_frame:=render.Picking_Buffer{resource=view.graph.frame,handle=view.slots[token.slot].frame,desc=view.graph.frame_desc}
                input,picking_error:=render.picking_graph_append(&graph,masked,pick_frame,picking_draws,pick_ids[i],pick_depth); assert(picking_error=={}); defer render.picking_graph_input_destroy(&input)
                // Existing helper's pass names are scoped by this host's actual camera namespace.
                for pass in input.passes { name:=fmt.aprintf("%s: %s",names[i],graph.passes[pass.index].name); delete(graph.passes[pass.index].name); graph.passes[pass.index].name=name }
                for value in input.buffers { found:=false; for previous in buffers { if previous.resource==value.resource { assert(previous.handle==value.handle); found=true; break } }; if !found { append(&buffers,value) } }
                for value in input.textures { found:=false; for previous in textures { if previous.resource==value.resource { assert(previous.handle==value.handle); found=true; break } }; if !found { append(&textures,value) } }
                append(&textures,gfx.Texture_Input{pick_ids[i],pick_handles[i][0]},gfx.Texture_Input{pick_depth,pick_handles[i][1]})
            }
            if variation==4 && i==0 {
                world,_:=app.scene_world_matrix(&owner,light); position:=km.mat4_extract_translation(world)
                hit:=render.overlay_hit_test(&mesh,positions[i],position-positions[i]); assert(hit.hit && hit.entity==light,"real billboard triangles lost entity hit identity")
            }
        }
        if variation!=0 { assert(render.overlay_native_destroy(&overlay)==.Busy,"CPU frame owners must keep the stationary overlay parent alive") }
        plan,compile_error:=gfx.graph_compile(&graph); assert(compile_error==.None); defer gfx.compiled_graph_destroy(&plan)
        submission,gpu_error,packet_error:=ops.submit(renderer,token,&graph,&plan,buffers[:],textures[:]); if gpu_error!=.None { fmt.println("Overlay submit:",backend,variation,gpu_error,packet_error) }; assert(gpu_error==.None && packet_error==.None)
        for &preparation in prepared { render.native_scene_prepared_accept(&preparation,submission) }
        // Drop public immutable parents while accepted work still owns them.
        for &upload in uploads { assert(render.overlay_frame_destroy(&overlay,&upload)==.None) }
        if variation==4 { assert(render.overlay_native_destroy(&overlay)==.None) }
        for &view,i in views {
            pixels:=capture(&view,captures,submission,view.graph.output); checked+=len(pixels.bytes)/4
            if variation==4 {
                identifiers:=capture(&view,captures,submission,pick_ids[i]); defer gfx.readback_data_destroy(&identifiers)
                words:=mem.slice_data_cast([]u32,identifiers.bytes); counts:[3]int
                for word in words { assert(word==0 || word==2 || word==3 || word==4,"billboard encodedID lost exact per-entity map"); if word>0 { counts[word-2]+=1 } }
                assert(counts[0]>20 && counts[1]>20 && counts[2]>20 && counts[0]+counts[1]+counts[2]<3000,"glyph alpha mask picking became full billboard quad or disappeared")
                hdr:=capture(&view,captures,submission,view.graph.color); defer gfx.readback_data_destroy(&hdr)
                expected:=km.color_to_array(km.color_to_linear({.5,.25,.75,1})); matched:=0
                for word,pixel in words { if word==authored_codes[i] {
                    equal:=true
                    for channel in 0..<3 { offset:=pixel*8+channel*2; bits:=u16(hdr.bytes[offset])|u16(hdr.bytes[offset+1])<<8; if abs(f32(transmute(f16)bits)-expected[channel])>.001 { equal=false } }
                    if equal { matched+=1 }
                } }
                assert(matched>20,"authored billboard RGB was not decoded to linear HDR before final tone mapping")
                fmt.println("Alpha-exact billboard IDs:",backend,i,counts,"linear tint pixels",matched)
            }
            if variation==0 { baseline[i]=pixels }
            else {
                reference:=&physical[i] if variation==2 else (&translated[i] if variation==8 else &baseline[i])
                changes[i]=changed_pixels(reference,&pixels,8)
                if variation==5 {
                    clip:=km.matrix_vector(frames[i].view_projection,km.vec4(km.Vec3{0,.5,0},1)); point:=clip/clip[3]
                    x:=int((point[0]*.5+.5)*256); y:=int((1-(point[1]*frames[i].ambient[3]*.5+.5))*192)
                    if backend=="vulkan" { y=int((point[1]*frames[i].ambient[3]*.5+.5)*192) }
                    interior:=0
                    for py in max(0,y-3)..<min(192,y+4) { for px in max(0,x-3)..<min(256,x+4) {
                        offset:=(py*256+px)*4; delta:=0; for channel in 0..<3 { delta+=abs(int(pixels.bytes[offset+channel])-int(baseline[i].bytes[offset+channel])) }; if delta>10 { interior+=1 }
                    } }
                    assert(interior>3,"always-visible gizmo disappeared inside the actual occluding cube")
                }
                if variation==4 { assert(changes[i]<3000,"light/fire glyph masks became full solid billboard squares") }
                if variation==1 { physical[i]=pixels } else if variation==5 { translated[i]=pixels } else { gfx.readback_data_destroy(&pixels) }
                assert(changes[i]>2,"real overlay geometry did not change actual viewport pixels")
            }
        }
        assert(ops.wait(renderer,submission)==.None)
        fmt.println("Native editor overlay:",backend,variation,changes)
    }
    assert(ops.release_exports(renderer,&graph)==.None)
    fmt.println("Native editor overlays PASS:",backend,"contacts",len(contacts),"four cameras",checked,"pixels")
}
main :: proc() {
    assert(len(os.args)==5,"shader compiler, Vulkan loader, font library, Box3D library")
    backing:=context.allocator; tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,backing); context.allocator=mem.tracking_allocator(&tracker)
    defer { context.allocator=backing; assert(len(tracker.allocation_map)==0); mem.tracking_allocator_destroy(&tracker) }
    _=NS.scoped_autoreleasepool()
    compiler:shader.Compiler; assert(shader.compiler_init(&compiler,os.args[1])==.None); defer assert(shader.compiler_destroy(&compiler)==.None)
    surface,error:=render.surface_shader_compile(&compiler); assert(error==.None); defer render.surface_shader_destroy(&surface)
    overlays,error_overlay:=render.overlay_shader_compile(&compiler); assert(error_overlay==.None); defer render.overlay_shader_destroy(&overlays)
    pick_shader,pick_compile_error:=render.picking_shader_compile(&compiler,masked=true); assert(pick_compile_error==.None); defer render.picking_shader_destroy(&pick_shader)
    fonts:render.UI_Font_System; assert(render.ui_font_init(&fonts,os.args[3],"resources")==.None); defer render.ui_font_destroy(&fonts)
    {
        renderer:metal.Renderer; assert(metal.renderer_init(&renderer)==.None); defer assert(metal.renderer_destroy(&renderer)==.None)
        ops:=render.GPU_Ops(metal.Renderer){metal.create_graphics_pipeline,metal.destroy_graphics_pipeline,metal.create_buffer_with_data,metal.destroy_buffer,metal.write_buffer,metal.create_texture,metal.destroy_texture,metal.acquire,metal.abort,metal.submit,metal.wait,metal.release_graph_exports,metal.create_pipeline,metal.destroy_pipeline,metal.create_sampler,metal.destroy_sampler}
        overlays_ops:=render.UI_GPU_Ops(metal.Renderer){create_pipeline=metal.create_graphics_pipeline,destroy_pipeline=metal.destroy_graphics_pipeline,create_buffer=metal.create_buffer_with_data,destroy_buffer=metal.destroy_buffer,create_texture=metal.create_texture_with_data,destroy_texture=metal.destroy_texture,create_sampler=metal.create_sampler,destroy_sampler=metal.destroy_sampler,create_target=metal.create_texture}
        exercise(&renderer,ops,overlays_ops,Capture(metal.Renderer){metal.graph_texture_source,metal.queue_texture_readback,metal.poll_texture_readback},render.surface_pipelines(&surface),"metal",&overlays,&fonts,&pick_shader,os.args[4])
        exercise_depth(&renderer,ops,overlays_ops,Capture(metal.Renderer){metal.graph_texture_source,metal.queue_texture_readback,metal.poll_texture_readback},render.surface_pipelines(&surface),"metal",&compiler,&overlays,&fonts,render.Particle_GPU_Ops(metal.Renderer){metal.create_pipeline,metal.destroy_pipeline,metal.create_graphics_pipeline,metal.destroy_graphics_pipeline,metal.create_buffer_with_data,metal.destroy_buffer,metal.write_buffer,metal.read_buffer})
        exercise_reload(&renderer,ops,overlays_ops,Capture(metal.Renderer){metal.graph_texture_source,metal.queue_texture_readback,metal.poll_texture_readback},&surface,"metal",&compiler,&overlays,&fonts,render.Particle_GPU_Ops(metal.Renderer){metal.create_pipeline,metal.destroy_pipeline,metal.create_graphics_pipeline,metal.destroy_graphics_pipeline,metal.create_buffer_with_data,metal.destroy_buffer,metal.write_buffer,metal.read_buffer})
    }
    {
        renderer:vulkan.Renderer; assert(vulkan.renderer_init(&renderer,validation=true,loader_path=os.args[2])==.None); defer { assert(renderer.validation_errors==0); assert(vulkan.renderer_destroy(&renderer)==.None) }
        ops:=render.GPU_Ops(vulkan.Renderer){vulkan.create_graphics_pipeline,vulkan.destroy_graphics_pipeline,vulkan.create_buffer_with_data,vulkan.destroy_buffer,vulkan.write_buffer,vulkan.create_texture,vulkan.destroy_texture,vulkan.acquire,vulkan.abort,vulkan.submit,vulkan.wait,vulkan.release_graph_exports,vulkan.create_pipeline,vulkan.destroy_pipeline,vulkan.create_sampler,vulkan.destroy_sampler}
        overlays_ops:=render.UI_GPU_Ops(vulkan.Renderer){create_pipeline=vulkan.create_graphics_pipeline,destroy_pipeline=vulkan.destroy_graphics_pipeline,create_buffer=vulkan.create_buffer_with_data,destroy_buffer=vulkan.destroy_buffer,create_texture=vulkan.create_texture_with_data,destroy_texture=vulkan.destroy_texture,create_sampler=vulkan.create_sampler,destroy_sampler=vulkan.destroy_sampler,create_target=vulkan.create_texture}
        exercise(&renderer,ops,overlays_ops,Capture(vulkan.Renderer){vulkan.graph_texture_source,vulkan.queue_texture_readback,vulkan.poll_texture_readback},render.surface_pipelines(&surface),"vulkan",&overlays,&fonts,&pick_shader,os.args[4])
        exercise_depth(&renderer,ops,overlays_ops,Capture(vulkan.Renderer){vulkan.graph_texture_source,vulkan.queue_texture_readback,vulkan.poll_texture_readback},render.surface_pipelines(&surface),"vulkan",&compiler,&overlays,&fonts,render.Particle_GPU_Ops(vulkan.Renderer){vulkan.create_pipeline,vulkan.destroy_pipeline,vulkan.create_graphics_pipeline,vulkan.destroy_graphics_pipeline,vulkan.create_buffer_with_data,vulkan.destroy_buffer,vulkan.write_buffer,vulkan.read_buffer})
        exercise_reload(&renderer,ops,overlays_ops,Capture(vulkan.Renderer){vulkan.graph_texture_source,vulkan.queue_texture_readback,vulkan.poll_texture_readback},&surface,"vulkan",&compiler,&overlays,&fonts,render.Particle_GPU_Ops(vulkan.Renderer){vulkan.create_pipeline,vulkan.destroy_pipeline,vulkan.create_graphics_pipeline,vulkan.destroy_graphics_pipeline,vulkan.create_buffer_with_data,vulkan.destroy_buffer,vulkan.write_buffer,vulkan.read_buffer})
    }
}
